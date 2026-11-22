//! What the owner knows of each capture beyond the file itself: its tags, whether it's a
//! favourite, and the program (and window) it was taken from. The capture service writes where a
//! capture came from when it keeps one; the editor's All captures adds tags and favourites, and
//! filters and searches by them.
//!
//! The library is a small file of its own (the host's; nothing here reads or writes it), keyed by
//! each capture's path. Everything in it comes from a file, so everything is clamped as it's read.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The most captures the library keeps anything for.
pub const MAX_CAPTURES: usize = 50_000;
/// The most tags one capture has.
pub const MAX_TAGS: usize = 32;
/// The longest tag, in characters.
pub const MAX_TAG_CHARS: usize = 40;
/// The longest program name or window title kept, in characters.
pub const MAX_SOURCE_CHARS: usize = 200;

/// What the library knows of one capture.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CaptureInfo {
    /// The owner's words for it, each once, in the order added.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub favorite: bool,
    /// The program whose window it was taken from (empty when it isn't known: an area of the
    /// screen, a whole screen).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub program: String,
    /// That window's title (a web page's title, in a browser).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,
}

impl CaptureInfo {
    /// Whether there's nothing to keep.
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && !self.favorite && self.program.is_empty() && self.title.is_empty()
    }

    /// Whether it has this tag (whatever its capitals).
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t.to_lowercase() == tag.to_lowercase())
    }

    /// Adds a tag (made tidy, see [`clean_tag`]); false when it's no tag, already there, or there
    /// are as many as can be.
    pub fn add_tag(&mut self, tag: &str) -> bool {
        let Some(tag) = clean_tag(tag) else { return false };
        if self.has_tag(&tag) || self.tags.len() >= MAX_TAGS {
            return false;
        }
        self.tags.push(tag);
        true
    }

    /// Takes a tag off (whatever its capitals); false when it wasn't there.
    pub fn remove_tag(&mut self, tag: &str) -> bool {
        let before = self.tags.len();
        let lower = tag.trim().to_lowercase();
        self.tags.retain(|t| t.to_lowercase() != lower);
        self.tags.len() != before
    }

    /// Every field within its limits, the tags tidy and each once.
    pub fn clamped(mut self) -> CaptureInfo {
        let tags = std::mem::take(&mut self.tags);
        for t in tags.iter().take(MAX_TAGS * 4) {
            self.add_tag(t);
        }
        self.program = clean_source(&self.program);
        self.title = clean_source(&self.title);
        self
    }

    /// Whether a word searched for (in lower case) is in its tags, program or title.
    pub fn mentions(&self, word: &str) -> bool {
        self.tags.iter().any(|t| t.to_lowercase().contains(word))
            || self.program.to_lowercase().contains(word)
            || self.title.to_lowercase().contains(word)
    }
}

/// A tag as kept: one line, spaces at its ends and runs of them inside taken out, no control
/// characters, at most [`MAX_TAG_CHARS`]. None when nothing is left.
pub fn clean_tag(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split(|c: char| c.is_whitespace() || c.is_control()).filter(|w| !w.is_empty()).collect();
    let tag: String = words.join(" ").chars().take(MAX_TAG_CHARS).collect();
    let tag = tag.trim().to_string();
    (!tag.is_empty()).then_some(tag)
}

/// A program name or a title as kept: one line, at most [`MAX_SOURCE_CHARS`].
pub fn clean_source(text: &str) -> String {
    let words: Vec<&str> = text.split(|c: char| c.is_whitespace() || c.is_control()).filter(|w| !w.is_empty()).collect();
    words.join(" ").chars().take(MAX_SOURCE_CHARS).collect::<String>().trim().to_string()
}

/// Every capture the library knows something of, by path.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Library {
    pub captures: BTreeMap<String, CaptureInfo>,
}

impl Library {
    /// Reads a library file; what can't be read is left out (all of it, if it isn't one).
    pub fn from_json(text: &str) -> Library {
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(text) else {
            return Library::default();
        };
        let mut lib = Library::default();
        if let Some(serde_json::Value::Object(captures)) = map.get("captures") {
            for (path, info) in captures.iter().take(MAX_CAPTURES) {
                if path.is_empty() || path.len() > 4096 {
                    continue;
                }
                if let Ok(info) = serde_json::from_value::<CaptureInfo>(info.clone()) {
                    lib.set(path, info.clamped());
                }
            }
        }
        lib
    }

    /// What's known of a capture (nothing, if it's not in the library).
    pub fn get(&self, path: &str) -> CaptureInfo {
        self.captures.get(path).cloned().unwrap_or_default()
    }

    /// Keeps what's known of a capture; nothing at all takes it out of the library. A capture
    /// past the most kept is left out.
    pub fn set(&mut self, path: &str, info: CaptureInfo) {
        if info.is_empty() {
            self.captures.remove(path);
        } else if self.captures.contains_key(path) || self.captures.len() < MAX_CAPTURES {
            self.captures.insert(path.to_string(), info);
        }
    }

    /// Every tag, each once (as first written), with how many of `paths` have it, the most used
    /// first and then by name.
    pub fn tags<'a>(&self, paths: impl IntoIterator<Item = &'a str>) -> Vec<(String, usize)> {
        count(paths.into_iter().filter_map(|p| self.captures.get(p)).flat_map(|i| i.tags.iter().cloned()))
    }

    /// Every program captures were taken from, each once, with how many of `paths` were taken
    /// from it, the most first and then by name.
    pub fn programs<'a>(&self, paths: impl IntoIterator<Item = &'a str>) -> Vec<(String, usize)> {
        count(paths.into_iter().filter_map(|p| self.captures.get(p)).map(|i| i.program.clone()).filter(|p| !p.is_empty()))
    }
}

/// Each name once (whatever its capitals; as first written), with how many times it came, the
/// most first and then by name.
fn count(names: impl Iterator<Item = String>) -> Vec<(String, usize)> {
    let mut by: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for n in names {
        let e = by.entry(n.to_lowercase()).or_insert_with(|| (n.clone(), 0));
        e.1 = e.1.saturating_add(1);
    }
    let mut out: Vec<(String, usize)> = by.into_values().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    out
}

/// What All captures shows: favourites only, one program's, one tag's, and the words searched for
/// (in the name, tags, program or window title).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub favorites: bool,
    pub program: Option<String>,
    pub tag: Option<String>,
    pub search: String,
}

impl Filter {
    /// Whether nothing is filtered out.
    pub fn is_empty(&self) -> bool {
        !self.favorites && self.program.is_none() && self.tag.is_none() && self.search.trim().is_empty()
    }

    /// Whether a capture (its file name and what's known of it) is shown.
    pub fn shows(&self, name: &str, info: &CaptureInfo) -> bool {
        if self.favorites && !info.favorite {
            return false;
        }
        if let Some(p) = &self.program
            && p.to_lowercase() != info.program.to_lowercase()
        {
            return false;
        }
        if let Some(t) = &self.tag
            && !info.has_tag(t)
        {
            return false;
        }
        let name = name.to_lowercase();
        self.search.to_lowercase().split_whitespace().all(|w| name.contains(w) || info.mentions(w))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_tidy_each_once_and_only_so_many() {
        let mut i = CaptureInfo::default();
        assert!(i.add_tag("  bug   report \n"));
        assert_eq!(i.tags, ["bug report"]);
        assert!(!i.add_tag("Bug Report"), "once, whatever its capitals");
        assert!(!i.add_tag(" \t "), "nothing is no tag");
        assert!(i.add_tag(&"x".repeat(100)));
        assert_eq!(i.tags.last().map(|t| t.chars().count()), Some(MAX_TAG_CHARS));
        for n in 0..100 {
            i.add_tag(&format!("t{n}"));
        }
        assert_eq!(i.tags.len(), MAX_TAGS);
        assert!(i.remove_tag("BUG REPORT"));
        assert!(!i.remove_tag("never"));
        assert!(!i.has_tag("bug report"));
    }

    #[test]
    fn a_library_file_is_read_whatever_is_in_it() {
        for junk in ["", "[]", "nonsense", r#"{"captures": 5}"#, r#"{"captures": {"a.png": 7, "": {"favorite": true}}}"#] {
            assert_eq!(Library::from_json(junk), Library::default(), "{junk}");
        }
        let text = format!(
            r#"{{"captures": {{"a.png": {{"tags": ["one", "ONE", "  two ", 5], "favorite": true, "program": "{}"}}, "b.png": {{"tags": ["x"]}}, "c.png": {{}}}}}}"#,
            "p".repeat(500)
        );
        let lib = Library::from_json(&text);
        // `a.png`'s tags weren't all words: it can't be read, so it's left out.
        assert!(lib.get("a.png").is_empty());
        assert_eq!(lib.get("b.png").tags, ["x"]);
        assert!(!lib.captures.contains_key("c.png"), "nothing known is not kept");
        let lib = Library::from_json(&format!(r#"{{"captures": {{"a.png": {{"tags": ["one", "ONE"], "program": "{}"}}}}}}"#, "p".repeat(500)));
        let a = lib.get("a.png");
        assert_eq!(a.tags, ["one"]);
        assert_eq!(a.program.chars().count(), MAX_SOURCE_CHARS);
        // And back.
        let json = serde_json::to_string(&lib).unwrap_or_default();
        assert_eq!(Library::from_json(&json), lib);
    }

    #[test]
    fn programs_and_tags_are_counted_most_first() {
        let mut lib = Library::default();
        let info = |program: &str, tags: &[&str]| CaptureInfo {
            program: program.into(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            ..CaptureInfo::default()
        };
        lib.set("1", info("Notes", &["work"]));
        lib.set("2", info("Browser", &["work", "home"]));
        lib.set("3", info("notes", &[]));
        lib.set("4", info("", &["Home"]));
        let all = ["1", "2", "3", "4"];
        assert_eq!(lib.programs(all), [("Notes".to_string(), 2), ("Browser".to_string(), 1)]);
        assert_eq!(lib.tags(all), [("home".to_string(), 2), ("work".to_string(), 2)]);
        assert_eq!(lib.tags(["1"]), [("work".to_string(), 1)], "only those asked about");
        lib.set("4", CaptureInfo::default());
        assert!(!lib.captures.contains_key("4"));
    }

    #[test]
    fn the_filter_shows_favourites_a_program_a_tag_and_what_is_searched_for() {
        let fav = CaptureInfo { favorite: true, program: "Notes".into(), tags: vec!["Plans".into()], title: "Shopping list".into() };
        let other = CaptureInfo { program: "Browser".into(), ..CaptureInfo::default() };
        let mut f = Filter::default();
        assert!(f.is_empty() && f.shows("a.png", &fav) && f.shows("b.png", &other));
        f.favorites = true;
        assert!(f.shows("a.png", &fav) && !f.shows("b.png", &other));
        f = Filter { program: Some("notes".into()), ..Filter::default() };
        assert!(f.shows("a.png", &fav) && !f.shows("b.png", &other));
        f = Filter { tag: Some("PLANS".into()), ..Filter::default() };
        assert!(f.shows("a.png", &fav) && !f.shows("b.png", &other));
        f = Filter { search: "shopping plans".into(), ..Filter::default() };
        assert!(f.shows("a.png", &fav), "the title and a tag");
        assert!(!f.shows("b.png", &other));
        f.search = "B.PNG".into();
        assert!(f.shows("b.png", &other), "the name");
    }
}
