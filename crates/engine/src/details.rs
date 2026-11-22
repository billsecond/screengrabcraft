//! Personal details found in the text read off a picture and hidden (`image.hideDetails`): email
//! addresses, phone numbers, card numbers and lines that look like a postal address, each covered
//! by a blur, a pixelate or a solid box of its own that the owner can still move or delete.
//!
//! The text comes from the host's reader (the lines and words it read, with their boxes), handed
//! in with the command or read with the reader the session was lent. Finding the details is plain
//! matching on that text, here. Faces aren't found: there is no model for them here.

use std::ops::Range;

use screengrabcraft_model::{Color, Kind, Pt, Rect, Shape, Style, TextLine};
use serde_json::{Value, json};

use crate::registry::{boolean, color};
use crate::session::Session;
use crate::{EngineError, Result};

/// A kind of personal detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Detail {
    Email,
    Phone,
    Card,
    /// A line that looks like part of a postal address (a house number and a street, a post
    /// office box, a postcode).
    Address,
}

impl Detail {
    pub const ALL: [Detail; 4] = [Detail::Email, Detail::Phone, Detail::Card, Detail::Address];

    /// Its name in a command's parameters and answer.
    pub fn name(self) -> &'static str {
        match self {
            Detail::Email => "email",
            Detail::Phone => "phone",
            Detail::Card => "card",
            Detail::Address => "address",
        }
    }
}

/// What covers a detail.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Cover {
    #[default]
    Blur,
    Pixelate,
    /// A box of solid colour.
    Box,
}

/// The details of the kinds `wanted` in a line of text, each with the characters of the line it
/// is (counted in characters, not bytes). An address is the whole line.
pub fn find(text: &str, wanted: &[Detail]) -> Vec<(Detail, Range<usize>)> {
    let c: Vec<char> = text.chars().take(screengrabcraft_model::text::MAX_CHARS).collect();
    let mut out = Vec::new();
    if wanted.contains(&Detail::Email) {
        out.extend(emails(&c).into_iter().map(|r| (Detail::Email, r)));
    }
    let emails_at: Vec<Range<usize>> = emails(&c);
    for run in number_runs(&c) {
        // Figures inside an email address are the address's.
        if emails_at.iter().any(|e| e.start < run.end && run.start < e.end) {
            continue;
        }
        let Some(chars) = c.get(run.clone()) else { continue };
        match number_kind(chars) {
            Some(Detail::Card) if wanted.contains(&Detail::Card) => out.push((Detail::Card, run)),
            Some(Detail::Phone) if wanted.contains(&Detail::Phone) => out.push((Detail::Phone, run)),
            _ => {}
        }
    }
    if wanted.contains(&Detail::Address) && address_like(&c) {
        out.push((Detail::Address, 0..c.len()));
    }
    out
}

// ---- email addresses ----

fn emails(c: &[char]) -> Vec<Range<usize>> {
    let local = |ch: char| ch.is_ascii_alphanumeric() || "._%+-'".contains(ch);
    let domain = |ch: char| ch.is_ascii_alphanumeric() || ch == '-' || ch == '.';
    let at = |k: usize| c.get(k).copied().unwrap_or(' ');
    let mut out: Vec<Range<usize>> = Vec::new();
    for (i, &ch) in c.iter().enumerate() {
        if ch != '@' && ch != '＠' {
            continue;
        }
        let mut a = i;
        while a > 0 && local(at(a - 1)) {
            a -= 1;
        }
        while a < i && at(a) == '.' {
            a += 1;
        }
        let mut b = i + 1;
        while b < c.len() && domain(at(b)) {
            b += 1;
        }
        while b > i + 1 && matches!(at(b - 1), '.' | '-') {
            b -= 1;
        }
        if a == i || b <= i + 1 {
            continue;
        }
        let dom: String = c.get(i + 1..b).unwrap_or_default().iter().collect();
        let labels: Vec<&str> = dom.split('.').collect();
        let top = labels.last().copied().unwrap_or("");
        let ok = labels.len() >= 2 && labels.iter().all(|l| !l.is_empty()) && top.len() >= 2 && top.chars().all(|ch| ch.is_ascii_alphabetic());
        if ok && out.last().is_none_or(|r| r.end <= a) {
            out.push(a..b);
        }
    }
    out
}

// ---- phone and card numbers ----

fn is_separator(ch: char) -> bool {
    matches!(ch, ' ' | '-' | '.' | '(' | ')' | '\u{a0}' | '\u{2010}' | '\u{2011}' | '\u{2013}')
}

/// Runs of figures with the separators phone and card numbers are written with, each starting at
/// a figure, a `+` or a `(` that follows no letter or figure, and ending at a figure that no letter
/// or figure follows.
fn number_runs(c: &[char]) -> Vec<Range<usize>> {
    let at = |k: usize| c.get(k).copied();
    let word = |k: Option<usize>| k.and_then(at).is_some_and(|ch| ch.is_alphanumeric());
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = at(i).unwrap_or(' ');
        let opens = ch.is_ascii_digit() || ((ch == '+' || ch == '(') && at(i + 1).is_some_and(|n| n.is_ascii_digit()));
        if !opens || word(i.checked_sub(1)) {
            i += 1;
            continue;
        }
        // As far as figures and separators go, never more than two separators in a row.
        let mut end = i + 1;
        let mut last_digit = if ch.is_ascii_digit() { Some(i) } else { None };
        let mut gap = 0;
        while let Some(n) = at(end) {
            if n.is_ascii_digit() {
                last_digit = Some(end);
                gap = 0;
            } else if is_separator(n) && gap < 2 {
                gap += 1;
            } else {
                break;
            }
            end += 1;
        }
        let Some(last) = last_digit else {
            i = end;
            continue;
        };
        let stop = last + 1;
        // Glued to a word ("4021abc"), it is part of that word.
        if !word(Some(stop)) && stop > i {
            out.push(i..stop);
        }
        i = end.max(i + 1);
    }
    out
}

/// Whether figures and separators are a card number (Luhn's check passes) or a phone number.
fn number_kind(chars: &[char]) -> Option<Detail> {
    let digits: Vec<u32> = chars.iter().filter_map(|c| c.to_digit(10)).collect();
    let mut groups: Vec<usize> = Vec::new();
    let mut seps: Vec<char> = Vec::new();
    let mut run = 0;
    for &ch in chars {
        if ch.is_ascii_digit() {
            run += 1;
        } else {
            if run > 0 {
                groups.push(run);
            }
            run = 0;
            if ch != '+' && !seps.contains(&ch) {
                seps.push(ch);
            }
        }
    }
    if run > 0 {
        groups.push(run);
    }
    let plus = chars.first() == Some(&'+');
    let n = digits.len();
    let plain = seps.iter().all(|s| matches!(s, ' ' | '-' | '\u{a0}'));
    if (13..=19).contains(&n) && plain && !plus && luhn(&digits) {
        return Some(Detail::Card);
    }
    if !(7..=15).contains(&n) {
        return None;
    }
    // Written with nothing between the figures: a phone number only when it starts with + or is
    // as long as one with its area code.
    if seps.is_empty() && !plus && !(10..=12).contains(&n) {
        return None;
    }
    let dots = seps.contains(&'.');
    let same = |want: &[usize]| groups.as_slice() == want;
    // Dates (2026-11-21, 21.11.2026, 1.2.2026) and years (1990-2000) are no phone numbers.
    let dated = groups.len() == 3
        && matches!((groups.first(), groups.get(1), groups.get(2)), (Some(4), Some(1..=2), Some(1..=2)) | (Some(1..=2), Some(1..=2), Some(4)));
    let years = same(&[4, 4]) && {
        let year = |s: &[u32]| matches!(s, [1, 9, ..] | [2, 0, ..]);
        year(digits.get(..4).unwrap_or_default()) && year(digits.get(4..).unwrap_or_default())
    };
    // Figures with dots are a version, an address on a network or a decimal unless grouped as a
    // phone number is (555.123.4567).
    let dotted_ok = !dots || (groups.len() == 3 && groups.iter().all(|g| (2..=4).contains(g)) && groups.last().is_some_and(|g| *g >= 3));
    if dated || years || !dotted_ok {
        return None;
    }
    Some(Detail::Phone)
}

/// Luhn's check, which every card number passes.
fn luhn(digits: &[u32]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 1 {
                let x = d * 2;
                if x > 9 { x - 9 } else { x }
            } else {
                d
            }
        })
        .sum();
    !digits.is_empty() && sum.is_multiple_of(10)
}

// ---- addresses ----

/// The words a street's name ends with (and the short ways they are written).
const STREETS: &[&str] = &[
    "street",
    "st",
    "road",
    "rd",
    "avenue",
    "ave",
    "av",
    "boulevard",
    "blvd",
    "lane",
    "ln",
    "drive",
    "dr",
    "way",
    "court",
    "ct",
    "place",
    "pl",
    "square",
    "sq",
    "terrace",
    "ter",
    "parkway",
    "pkwy",
    "highway",
    "hwy",
    "close",
    "crescent",
    "cres",
    "gardens",
    "mews",
    "row",
    "grove",
    "circle",
    "cir",
    "alley",
    "trail",
    "walk",
    "plaza",
    "suite",
    "ste",
    "apt",
    "apartment",
    "unit",
];

/// Street names that end in a word for "street", written before the house number.
const STREETS_FIRST: &[&str] = &["straße", "strasse", "str", "weg", "gasse", "platz", "allee", "laan", "gatan", "vej", "veien"];

fn address_like(c: &[char]) -> bool {
    let text: String = c.iter().collect();
    let tokens: Vec<&str> = text.split(|ch: char| ch.is_whitespace() || ch == ',').filter(|t| !t.is_empty()).collect();
    let tok = |k: usize| tokens.get(k).copied().unwrap_or("");
    let bare = |t: &str| t.trim_matches(['.', ';', ':']).to_lowercase();
    let house = |t: &str| {
        let t = t.trim_end_matches([',', '.']);
        let digits = t.chars().take_while(char::is_ascii_digit).count();
        let rest: Vec<char> = t.chars().skip(digits).collect();
        (1..=6).contains(&digits) && (rest.is_empty() || (rest.len() == 1 && rest.iter().all(|ch| ch.is_ascii_alphabetic())))
    };
    let capital = |t: &str| t.chars().next().is_some_and(|ch| ch.is_uppercase() || ch.is_ascii_digit());
    for i in 0..tokens.len() {
        let t = tok(i);
        // A house number, a name and a street word: "221B Baker Street", "1600 Pennsylvania Ave NW".
        if house(t) {
            for k in 1..=4 {
                let w = tok(i + k);
                if w.is_empty() || !capital(w) {
                    break;
                }
                if k >= 2 && STREETS.contains(&bare(w).as_str()) {
                    return true;
                }
            }
        }
        // A street word first, then the number: "Hauptstraße 5".
        let low = bare(t);
        if capital(t) && STREETS_FIRST.iter().any(|s| low.ends_with(s)) && house(tok(i + 1)) {
            return true;
        }
        // A post office box: "PO Box 123", "P.O. Box 123".
        if matches!(low.replace('.', "").as_str(), "po" | "pob") && bare(tok(i + 1)) == "box" && house(tok(i + 2)) {
            return true;
        }
        // A town, its state and ZIP code: "Springfield, IL 62704".
        let state = t.len() == 2 && t.chars().all(|ch| ch.is_ascii_uppercase());
        let zip = |z: &str| {
            let z: Vec<char> = z.chars().collect();
            let five = z.iter().take(5).all(char::is_ascii_digit);
            five && (z.len() == 5 || (z.len() == 10 && z.get(5) == Some(&'-') && z.iter().skip(6).all(char::is_ascii_digit)))
        };
        let comma_before = i > 0 && text.split(|ch: char| ch.is_whitespace()).any(|w| w.ends_with(',') && w.trim_end_matches(',') == tok(i - 1));
        if state && comma_before && zip(tok(i + 1)) {
            return true;
        }
        // A British postcode ("SW1A 1AA") or a Canadian one ("K1A 0B1").
        if postcode_out(t) && postcode_in(tok(i + 1)) {
            return true;
        }
    }
    false
}

/// The first half of a British postcode (`SW1A`, `M1`, `EC1A`) or of a Canadian one (`K1A`).
fn postcode_out(t: &str) -> bool {
    let c: Vec<char> = t.chars().collect();
    let letters = c.iter().take_while(|ch| ch.is_ascii_uppercase()).count();
    let rest = c.get(letters..).unwrap_or_default();
    match (letters, rest) {
        (1 | 2, [d]) | (1 | 2, [d, _]) if d.is_ascii_digit() => rest.iter().all(|ch| ch.is_ascii_digit() || ch.is_ascii_uppercase()),
        _ => false,
    }
}

/// The second half of a British postcode (`1AA`) or of a Canadian one (`0B1`).
fn postcode_in(t: &str) -> bool {
    let c: Vec<char> = t.trim_end_matches(['.', ',']).chars().collect();
    match c.as_slice() {
        [d, a, b] if d.is_ascii_digit() && a.is_ascii_uppercase() => b.is_ascii_uppercase() || b.is_ascii_digit(),
        _ => false,
    }
}

// ---- where a detail is on the picture ----

/// The box round the characters `range` of `line`: from its words where the reader gave them
/// (part of a word by its share of the word's width, a letter's width to spare), else by the
/// share of the line.
pub fn span_box(line: &TextLine, range: Range<usize>) -> Rect {
    let text: Vec<char> = line.text.chars().collect();
    let mut from = 0usize;
    let mut found: Option<Rect> = None;
    for w in &line.words {
        let wc: Vec<char> = w.text.chars().collect();
        let Some(start) = position(&text, &wc, from) else { continue };
        let end = start + wc.len();
        from = end;
        if start < range.end && range.start < end {
            let n = wc.len().max(1) as f32;
            let a = range.start.saturating_sub(start).saturating_sub(1) as f32;
            let b = ((range.end.min(end) - start) + 1).min(wc.len()) as f32;
            let r = Rect::new(w.rect.x + w.rect.w * a / n, w.rect.y, w.rect.w * (b - a).max(0.0) / n, w.rect.h);
            found = Some(found.map_or(r, |f| f.union(r)));
        }
    }
    if let Some(r) = found {
        return r;
    }
    let n = text.len().max(1) as f32;
    let (a, b) = (range.start.saturating_sub(1).min(text.len()) as f32, range.end.saturating_add(1).min(text.len()) as f32);
    Rect::new(line.rect.x + line.rect.w * a / n, line.rect.y, line.rect.w * (b - a).max(0.0) / n, line.rect.h)
}

/// Where `word` is in `text` at `from` or after.
fn position(text: &[char], word: &[char], from: usize) -> Option<usize> {
    if word.is_empty() || word.len() > text.len() {
        return None;
    }
    (from..=text.len() - word.len()).find(|&k| text.get(k..k + word.len()) == Some(word))
}

// ---- the command ----

/// The kinds asked for: `find`, or every kind.
fn wanted(p: &Value) -> Result<Vec<Detail>> {
    match p.get("find") {
        None | Some(Value::Null) => Ok(Detail::ALL.to_vec()),
        Some(Value::Array(a)) => a
            .iter()
            .take(Detail::ALL.len() * 2)
            .map(|v| serde_json::from_value::<Detail>(v.clone()))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| EngineError::Params("`find` is a list of \"email\", \"phone\", \"card\" and \"address\"".to_string())),
        Some(_) => Err(EngineError::Params("`find` is a list of \"email\", \"phone\", \"card\" and \"address\"".to_string())),
    }
}

fn area_param(p: &Value) -> Result<Option<Rect>> {
    match p.get("area") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => serde_json::from_value::<Rect>(v.clone())
            .map(|r| Some(r.finite()))
            .map_err(|_| EngineError::Params("`area` is {\"x\", \"y\", \"w\", \"h\"}".to_string())),
    }
}

/// The lines the command works on: given, or read off the picture as shown (or the area of it)
/// with the reader the session was lent.
pub(crate) fn lines_param(s: &Session, p: &Value, area: Option<Rect>) -> Result<Vec<TextLine>> {
    match p.get("lines") {
        Some(Value::Array(a)) => a
            .iter()
            .take(screengrabcraft_model::text::MAX_LINES)
            .map(|v| serde_json::from_value::<TextLine>(v.clone()).map(TextLine::sanitized))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| EngineError::Params(format!("`lines` is a list of {{\"text\", \"rect\", \"words\"?}}: {e}"))),
        None | Some(Value::Null) => {
            let shown = screengrabcraft_render::render(&s.doc).ok_or_else(|| EngineError::Failed("the picture could not be drawn".to_string()))?;
            let Some(a) = area else {
                return s.reader.read(&shown).map_err(|e| EngineError::Failed(format!("reading the text: {e}; or give the `lines` read")));
            };
            let (x, y, w, h) =
                a.pixel_bounds(shown.width(), shown.height()).ok_or_else(|| EngineError::Params("the area is outside the picture".to_string()))?;
            let piece = shown.crop(x, y, w, h).ok_or_else(|| EngineError::Params("the area is outside the picture".to_string()))?;
            let lines = s.reader.read(&piece).map_err(|e| EngineError::Failed(format!("reading the text: {e}; or give the `lines` read")))?;
            Ok(lines.iter().map(|l| l.translated(Pt::new(x as f32, y as f32))).collect())
        }
        Some(_) => Err(EngineError::Params("`lines` is a list of {\"text\", \"rect\", \"words\"?}".to_string())),
    }
}

/// Hide Personal Details: every detail of the kinds asked for found in the text, covered.
pub(crate) fn hide(s: &mut Session, p: &Value) -> Result<Value> {
    let wanted = wanted(p)?;
    let cover = match p.get("cover") {
        None | Some(Value::Null) => Cover::default(),
        Some(v) => {
            serde_json::from_value::<Cover>(v.clone()).map_err(|_| EngineError::Params("`cover` is \"blur\", \"pixelate\" or \"box\"".to_string()))?
        }
    };
    let colour = color(p, "color", Color::BLACK)?;
    let area = area_param(p)?;
    let select = boolean(p, "select", true);
    let lines = lines_param(s, p, area)?;
    let picture = s.doc.bounds();
    // Every detail's box, a little larger than its letters, on the picture (and in the area).
    let mut found: Vec<(Detail, Rect)> = Vec::new();
    for line in &lines {
        for (kind, range) in find(&line.text, &wanted) {
            let r = if kind == Detail::Address { line.rect } else { span_box(line, range) }.finite();
            if area.is_some_and(|a| !a.contains(r.center())) {
                continue;
            }
            let pad = (r.h * 0.2).clamp(2.0, 12.0);
            let Some(r) = r.inflate(pad).intersect(picture) else { continue };
            if r.w >= 1.0 && r.h >= 1.0 {
                found.push((kind, r));
            }
        }
    }
    // A detail inside another's cover (a phone number on an address line) needs none of its own.
    found.sort_by(|a, b| (b.1.w * b.1.h).total_cmp(&(a.1.w * a.1.h)));
    let mut kept: Vec<(Detail, Rect)> = Vec::new();
    for (kind, r) in found {
        if !kept.iter().any(|(_, k)| k.inflate(1.0).encloses(r)) {
            kept.push((kind, r));
        }
    }
    kept.sort_by(|a, b| (a.1.y, a.1.x).partial_cmp(&(b.1.y, b.1.x)).unwrap_or(std::cmp::Ordering::Equal));
    let mut ids = Vec::new();
    let mut counts = serde_json::Map::new();
    for d in Detail::ALL {
        counts.insert(d.name().to_string(), json!(0));
    }
    for (kind, rect) in &kept {
        let (shape, style) = match cover {
            Cover::Blur => (Shape::Blur { rect: *rect }, Kind::Blur.default_style()),
            Cover::Pixelate => (Shape::Pixelate { rect: *rect }, Kind::Pixelate.default_style()),
            Cover::Box => {
                let style = Style { stroke: colour, fill: colour, width: 1.0, shadow: false, ..Kind::Rect.default_style() };
                (Shape::Rect { rect: *rect, radius: 0.0 }, style)
            }
        };
        ids.push(s.doc.add(shape, style)?);
        if let Some(n) = counts.get_mut(kind.name()) {
            *n = json!(n.as_u64().unwrap_or(0) + 1);
        }
    }
    if select && !ids.is_empty() {
        s.selection = ids.clone();
    }
    let mut out = json!({ "hidden": ids.len(), "ids": ids, "lines": lines.len() });
    if let Value::Object(map) = &mut out {
        map.extend(counts);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use screengrabcraft_model::{Document, Raster, TextWord};

    use super::*;

    fn kinds(text: &str) -> Vec<(Detail, String)> {
        let c: Vec<char> = text.chars().collect();
        find(text, &Detail::ALL).into_iter().map(|(k, r)| (k, c.get(r).unwrap().iter().collect())).collect()
    }

    #[test]
    fn email_addresses_are_found_inside_words_and_sentences() {
        assert_eq!(kinds("Write to jane.doe+news@mail.example.co.uk today."), vec![(Detail::Email, "jane.doe+news@mail.example.co.uk".to_string())]);
        assert_eq!(kinds("Email:bob@example.com"), vec![(Detail::Email, "bob@example.com".to_string())]);
        for not in ["@home", "me@", "a@b", "x@y.z", "price @ 5.00", "user@host.c0m"] {
            assert!(kinds(not).iter().all(|(k, _)| *k != Detail::Email), "{not}");
        }
        // The figures of an address are the address's, not a phone number.
        assert_eq!(kinds("id 5551234567@sms.example.net").len(), 1);
    }

    #[test]
    fn phone_numbers_are_found_and_dates_times_and_prices_are_not() {
        for phone in
            ["(555) 123-4567", "555-123-4567", "+1 555 123 4567", "+44 20 7946 0958", "0161 496 0000", "555.123.4567", "5551234567", "555-1234"]
        {
            assert_eq!(kinds(&format!("Call {phone} now")), vec![(Detail::Phone, phone.to_string())], "{phone}");
        }
        for not in [
            "2026-11-21",
            "21.11.2026",
            "12:30",
            "19.95",
            "Order 4021",
            "1990-2000",
            "version 1.2.3",
            "192.168.1.1",
            "3.14159265",
            "1234567",
            "4021abc5551234567",
            "page 12 of 300",
        ] {
            assert!(kinds(not).is_empty(), "{not}: {:?}", kinds(not));
        }
    }

    #[test]
    fn card_numbers_pass_luhns_check() {
        assert_eq!(kinds("Card 4111 1111 1111 1111 exp 12/29"), vec![(Detail::Card, "4111 1111 1111 1111".to_string())]);
        assert_eq!(kinds("5500-0000-0000-0004"), vec![(Detail::Card, "5500-0000-0000-0004".to_string())]);
        assert_eq!(kinds("378282246310005"), vec![(Detail::Card, "378282246310005".to_string())]);
        // Sixteen figures that fail the check are not a card (nor a phone number: too long).
        assert!(kinds("4111 1111 1111 1112").is_empty());
    }

    #[test]
    fn lines_that_look_like_an_address_are_found() {
        for line in [
            "221B Baker Street",
            "1600 Pennsylvania Avenue NW",
            "Ship to: 742 Evergreen Terrace, Springfield",
            "Springfield, IL 62704",
            "London SW1A 1AA",
            "Ottawa ON K1A 0B1",
            "P.O. Box 1234",
            "Hauptstraße 5",
        ] {
            assert!(kinds(line).iter().any(|(k, _)| *k == Detail::Address), "{line}");
        }
        for not in ["Click 2 more times", "Step 3: press Start", "4 ways to drive faster", "The Way We Were", "Version 10 of Windows", "OK", ""] {
            assert!(kinds(not).iter().all(|(k, _)| *k != Detail::Address), "{not}");
        }
    }

    #[test]
    fn only_the_kinds_asked_for_are_found() {
        let line = "jo@example.com 555-123-4567";
        assert_eq!(find(line, &[Detail::Phone]).len(), 1);
        assert_eq!(find(line, &[Detail::Email]).len(), 1);
        assert!(find(line, &[Detail::Card, Detail::Address]).is_empty());
    }

    #[test]
    fn a_detail_is_boxed_by_its_words_or_its_share_of_the_line() {
        let mut line = TextLine::new("Mail bob@example.com", Rect::new(0.0, 10.0, 200.0, 20.0));
        // No words: by its share of the line, a letter to spare.
        let r = span_box(&line, 5..20);
        assert!((r.x - 40.0).abs() < 0.01 && (r.right() - 200.0).abs() < 0.01, "{r:?}");
        // With words: the second word's box.
        line.words = vec![
            TextWord { text: "Mail".into(), rect: Rect::new(0.0, 10.0, 40.0, 20.0) },
            TextWord { text: "bob@example.com".into(), rect: Rect::new(60.0, 10.0, 140.0, 20.0) },
        ];
        assert_eq!(span_box(&line, 5..20), Rect::new(60.0, 10.0, 140.0, 20.0));
        // Part of a word: its share, a letter to spare each side.
        let glued = TextLine {
            text: "Email:bob@example.com".into(),
            rect: Rect::new(0.0, 0.0, 210.0, 20.0),
            words: vec![TextWord { text: "Email:bob@example.com".into(), rect: Rect::new(0.0, 0.0, 210.0, 20.0) }],
        };
        let r = span_box(&glued, 6..21);
        assert!((r.x - 50.0).abs() < 0.01 && (r.right() - 210.0).abs() < 0.01, "{r:?}");
    }

    fn session() -> Session {
        Session::new(Document::new(Raster::filled(400, 200, Color::WHITE).unwrap()))
    }

    fn lines() -> Value {
        json!([
            {"text": "Name: Jo Bloggs", "rect": {"x": 10, "y": 10, "w": 150, "h": 16}},
            {"text": "jo@example.com", "rect": {"x": 10, "y": 40, "w": 140, "h": 16}},
            {"text": "Tel (555) 123-4567", "rect": {"x": 10, "y": 70, "w": 180, "h": 16}},
            {"text": "221B Baker Street, London", "rect": {"x": 10, "y": 100, "w": 250, "h": 16}},
        ])
    }

    #[test]
    fn details_are_covered_by_annotations_that_can_be_moved_or_deleted() {
        let mut s = session();
        let out = s.execute("image.hideDetails", &json!({"lines": lines()})).unwrap();
        assert_eq!(out["hidden"], 3, "{out}");
        assert_eq!(
            (out["email"].as_u64(), out["phone"].as_u64(), out["address"].as_u64(), out["card"].as_u64()),
            (Some(1), Some(1), Some(1), Some(0))
        );
        assert_eq!(s.doc.objects.len(), 3);
        assert!(s.doc.objects.iter().all(|o| matches!(o.shape, Shape::Blur { .. })));
        assert_eq!(s.selected().len(), 3, "left selected, to see and delete");
        // Each covers its line, a little larger.
        let email = s.doc.objects.first().unwrap().shape.rect().unwrap();
        assert!(email.y < 40.0 && email.bottom() > 56.0 && email.x < 10.0, "{email:?}");
        // One undo step takes them all away.
        assert_eq!(s.undo_label(), Some("Hide Personal Details"));
        s.undo();
        assert!(s.doc.objects.is_empty());
    }

    #[test]
    fn the_cover_can_be_pixelated_or_a_solid_box() {
        let mut s = session();
        s.execute("image.hideDetails", &json!({"lines": lines(), "cover": "pixelate", "find": ["email"]})).unwrap();
        assert!(matches!(s.doc.objects.as_slice(), [o] if matches!(o.shape, Shape::Pixelate { .. })));
        let mut s = session();
        s.execute("image.hideDetails", &json!({"lines": lines(), "cover": "box", "color": "#202020", "find": ["phone"]})).unwrap();
        let o = s.doc.objects.first().unwrap();
        assert!(matches!(o.shape, Shape::Rect { .. }));
        assert_eq!((o.style.fill, o.style.stroke), (Color::rgb(0x20, 0x20, 0x20), Color::rgb(0x20, 0x20, 0x20)));
    }

    #[test]
    fn nothing_found_changes_nothing_and_is_no_undo_step() {
        let mut s = session();
        let out = s.execute("image.hideDetails", &json!({"lines": [{"text": "Hello world", "rect": {"x": 0, "y": 0, "w": 50, "h": 10}}]})).unwrap();
        assert_eq!(out["hidden"], 0);
        assert!(!s.can_undo());
    }

    #[test]
    fn only_the_area_is_hidden() {
        let mut s = session();
        let out = s.execute("image.hideDetails", &json!({"lines": lines(), "area": {"x": 0, "y": 60, "w": 400, "h": 30}})).unwrap();
        assert_eq!(out["hidden"], 1);
        assert_eq!(out["phone"], 1);
    }

    struct Fake;

    impl crate::ReadText for Fake {
        fn read(&self, picture: &Raster) -> std::result::Result<Vec<TextLine>, String> {
            Ok(vec![TextLine::new("Call 555-123-4567", Rect::new(2.0, 2.0, picture.width() as f32 - 4.0, 14.0))])
        }
    }

    #[test]
    fn without_the_lines_the_reader_lent_reads_the_picture() {
        let mut s = session();
        assert!(s.execute("image.hideDetails", &json!({})).is_err(), "no reader, no lines");
        s.reader.set(Some(std::sync::Arc::new(Fake)));
        let out = s.execute("image.hideDetails", &json!({})).unwrap();
        assert_eq!(out["phone"], 1);
        // Read off an area, the lines are put where the area is.
        let mut s = session();
        s.reader.set(Some(std::sync::Arc::new(Fake)));
        s.execute("image.hideDetails", &json!({"area": {"x": 100, "y": 100, "w": 200, "h": 50}})).unwrap();
        let r = s.doc.objects.first().unwrap().shape.rect().unwrap();
        assert!(r.x >= 100.0 && r.y >= 95.0, "{r:?}");
    }

    #[test]
    fn junk_lines_are_refused_or_kept_in_bounds() {
        let mut s = session();
        assert!(s.execute("image.hideDetails", &json!({"lines": "x"})).is_err());
        assert!(s.execute("image.hideDetails", &json!({"lines": [{"text": 5}]})).is_err());
        assert!(s.execute("image.hideDetails", &json!({"lines": lines(), "find": ["faces"]})).is_err());
        assert!(s.execute("image.hideDetails", &json!({"lines": lines(), "cover": "paint"})).is_err());
        let huge = json!([{"text": "5".repeat(100_000), "rect": {"x": -1e30, "y": 1e30, "w": 1e38, "h": -4}}]);
        assert!(s.execute("image.hideDetails", &json!({ "lines": huge })).is_ok());
        let far = json!([{"text": "jo@example.com", "rect": {"x": 5000, "y": 5000, "w": 100, "h": 10}}]);
        assert_eq!(s.execute("image.hideDetails", &json!({ "lines": far })).unwrap()["hidden"], 0);
    }
}
