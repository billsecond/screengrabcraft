//! Text read off a picture: its lines and their words, each with where it is on the picture (in
//! its pixels). The host reads it (Grab Text, on this computer); the commands that find personal
//! details or edit text in place take it as it was read.

use serde::{Deserialize, Serialize};

use crate::geom::{Pt, Rect};

/// The most lines a picture gives (a page of tiny print is still a page).
pub const MAX_LINES: usize = 5000;
/// The most words a line keeps.
pub const MAX_WORDS: usize = 500;
/// The most characters of a line (or a word) kept.
pub const MAX_CHARS: usize = 4000;

/// A word read off a picture, and its box.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextWord {
    pub text: String,
    pub rect: Rect,
}

/// A line of text read off a picture, its box, and (when the reader gives them) its words.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextLine {
    pub text: String,
    pub rect: Rect,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<TextWord>,
}

impl TextLine {
    /// A line without its words.
    pub fn new(text: impl Into<String>, rect: Rect) -> TextLine {
        TextLine { text: text.into(), rect, words: Vec::new() }
    }

    /// The same line moved by `d` (a line read off an area, put where it is on the picture).
    pub fn translated(&self, d: Pt) -> TextLine {
        TextLine {
            text: self.text.clone(),
            rect: self.rect.translate(d),
            words: self.words.iter().map(|w| TextWord { text: w.text.clone(), rect: w.rect.translate(d) }).collect(),
        }
    }

    /// The same line with every length divided by `f` (a line read off a picture made larger).
    pub fn shrunk(mut self, f: f32) -> TextLine {
        if !(f.is_finite() && f > 0.0) {
            return self;
        }
        let s = |r: Rect| Rect::new(r.x / f, r.y / f, r.w / f, r.h / f);
        self.rect = s(self.rect);
        for w in &mut self.words {
            w.rect = s(w.rect);
        }
        self
    }

    /// The same line kept within bounds: finite boxes, at most [`MAX_CHARS`] characters and
    /// [`MAX_WORDS`] words (for a line that came from outside).
    pub fn sanitized(self) -> TextLine {
        let cut = |t: String| if t.chars().count() > MAX_CHARS { t.chars().take(MAX_CHARS).collect() } else { t };
        TextLine {
            text: cut(self.text),
            rect: self.rect.finite(),
            words: self.words.into_iter().take(MAX_WORDS).map(|w| TextWord { text: cut(w.text), rect: w.rect.finite() }).collect(),
        }
    }
}

/// The lines, one to a line, in the order they were read.
pub fn joined(lines: &[TextLine]) -> String {
    let mut s = String::new();
    for l in lines {
        if !s.is_empty() {
            s.push('\n');
        }
        s.push_str(l.text.trim_end());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_joined_one_to_a_line() {
        let l = |t: &str| TextLine::new(t, Rect::new(0.0, 0.0, 1.0, 1.0));
        assert_eq!(joined(&[l("Hello world  "), l("Second")]), "Hello world\nSecond");
        assert_eq!(joined(&[]), "");
    }

    #[test]
    fn a_line_moves_and_shrinks_with_its_words() {
        let mut line = TextLine::new("a b", Rect::new(10.0, 20.0, 40.0, 8.0));
        line.words = vec![TextWord { text: "a".into(), rect: Rect::new(10.0, 20.0, 8.0, 8.0) }];
        let moved = line.translated(Pt::new(5.0, -5.0));
        assert_eq!(moved.rect, Rect::new(15.0, 15.0, 40.0, 8.0));
        assert_eq!(moved.words[0].rect, Rect::new(15.0, 15.0, 8.0, 8.0));
        let half = line.shrunk(2.0);
        assert_eq!(half.rect, Rect::new(5.0, 10.0, 20.0, 4.0));
        assert_eq!(half.words[0].rect, Rect::new(5.0, 10.0, 4.0, 4.0));
        // Junk changes nothing.
        assert_eq!(half.clone().shrunk(f32::NAN), half);
    }

    #[test]
    fn a_line_from_outside_is_kept_within_bounds() {
        let mut line = TextLine::new("é".repeat(MAX_CHARS * 2), Rect::new(f32::NAN, 0.0, 1.0, 1.0));
        line.words = (0..MAX_WORDS * 2).map(|_| TextWord { text: "w".into(), rect: Rect::default() }).collect();
        let s = line.sanitized();
        assert_eq!(s.text.chars().count(), MAX_CHARS);
        assert_eq!(s.words.len(), MAX_WORDS);
        assert!(s.rect.x.is_finite());
    }

    #[test]
    fn words_are_optional_in_json() {
        let l: TextLine = serde_json::from_str(r#"{"text": "hi", "rect": {"x": 1, "y": 2, "w": 3, "h": 4}}"#).unwrap();
        assert!(l.words.is_empty());
        assert!(!serde_json::to_string(&l).unwrap().contains("words"));
    }
}
