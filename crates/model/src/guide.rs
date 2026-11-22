//! A how-to guide made by step capture: each click while it ran took a picture of the window
//! clicked, with the click marked, and is a numbered step with words of its own. The guide is a
//! small file (`.sgguide`, JSON) beside a folder holding the steps' pictures; it's edited
//! (reordered, steps taken out, words changed) and saved as a PDF, a word-processor document,
//! slides, a web page or Markdown.
//!
//! Everything in it may come from a file, so everything is clamped as it's read.

use serde::{Deserialize, Serialize};

/// A guide file's extension.
pub const GUIDE_EXT: &str = "sgguide";
/// The most steps a guide has.
pub const MAX_STEPS: usize = 500;
/// The longest a step's words or a guide's title get, in characters.
pub const MAX_TEXT: usize = 2000;
pub const MAX_TITLE: usize = 200;

/// One step: its picture (a file in the guide's folder), its words, and where the click was on
/// the picture.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GuideStep {
    /// The picture's file name, in the guide's folder (a name only, never a path).
    pub picture: String,
    pub text: String,
    /// Where the click was, in the picture's pixels.
    pub click: Option<(u32, u32)>,
}

/// A how-to guide.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Guide {
    pub title: String,
    pub steps: Vec<GuideStep>,
}

/// Whether a picture's file name is a plain name in the guide's folder (no folder of its own, no
/// way out of it).
pub fn plain_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 255 && !name.contains(['/', '\\', ':']) && name != "." && name != ".." && !name.chars().any(char::is_control)
}

/// Words as kept: at most `max` characters, no control characters but line breaks.
pub fn clean_text(text: &str, max: usize) -> String {
    text.chars().filter(|c| *c == '\n' || !c.is_control()).take(max).collect()
}

impl Guide {
    /// Reads a guide file; a step that can't be read, or whose picture isn't a plain name, is
    /// left out.
    pub fn from_json(text: &str) -> Option<Guide> {
        let serde_json::Value::Object(map) = serde_json::from_str::<serde_json::Value>(text).ok()? else { return None };
        let title = map.get("title").and_then(serde_json::Value::as_str).map(|t| clean_text(t, MAX_TITLE)).unwrap_or_default();
        let mut steps = Vec::new();
        if let Some(serde_json::Value::Array(list)) = map.get("steps") {
            for s in list.iter().take(MAX_STEPS) {
                if let Ok(mut step) = serde_json::from_value::<GuideStep>(s.clone())
                    && plain_name(&step.picture)
                {
                    step.text = clean_text(&step.text, MAX_TEXT);
                    steps.push(step);
                }
            }
        }
        Some(Guide { title: title.replace('\n', " "), steps })
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// The words a step starts with: what was clicked, in which program's window.
pub fn default_text(title: &str, program: &str) -> String {
    let title = clean_text(title, 120).replace('\n', " ");
    let program = clean_text(program, 60).replace('\n', " ");
    match (title.trim().is_empty(), program.trim().is_empty()) {
        (false, false) if !title.contains(program.trim()) => format!("Click in \"{}\" ({})", title.trim(), program.trim()),
        (false, _) => format!("Click in \"{}\"", title.trim()),
        (true, false) => format!("Click in {}", program.trim()),
        (true, true) => "Click here".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_guide_file_is_read_whatever_is_in_it() {
        for junk in ["", "[]", "{", r#"{"steps": 4}"#] {
            assert!(Guide::from_json(junk).is_none_or(|g| g.steps.is_empty()), "{junk}");
        }
        let text = r#"{"title": "Log in\n", "steps": [
            {"picture": "step-01.png", "text": "Open it", "click": [10, 20]},
            {"picture": "../../secret.png", "text": "out of the folder"},
            {"picture": "C:\\x.png"},
            {"picture": "step-02.png", "click": "nowhere"},
            {"picture": "step-03.png", "text": "bell\u0007 rang"}
        ]}"#;
        let g = Guide::from_json(text).unwrap();
        assert_eq!(g.title, "Log in ");
        assert_eq!(g.steps.iter().map(|s| s.picture.as_str()).collect::<Vec<_>>(), ["step-01.png", "step-03.png"]);
        assert_eq!(g.steps.first().and_then(|s| s.click), Some((10, 20)));
        assert_eq!(g.steps.get(1).map(|s| s.text.as_str()), Some("bell rang"));
        assert_eq!(Guide::from_json(&g.to_json()), Some(g));
    }

    #[test]
    fn a_step_starts_by_saying_what_was_clicked() {
        assert_eq!(default_text("Settings", "Notes"), "Click in \"Settings\" (Notes)");
        assert_eq!(default_text("Notes - Settings", "Notes"), "Click in \"Notes - Settings\"");
        assert_eq!(default_text("", "Notes"), "Click in Notes");
        assert_eq!(default_text(" ", ""), "Click here");
    }
}
