//! What the owner makes in the video editor to use again: templates (the layers' items and their
//! look, kept under a name and put on another recording) and themes (colours and a face for the
//! titles, lower thirds and callouts). Both are kept by the host as files of their own; here they
//! are made, read back safely and used.

use serde::{Deserialize, Serialize};

use crate::layers::{self, Item, Layer, MediaItem, TextFont, TextStyle, What};
use crate::timeline::{EditError, Timeline};

/// The largest template or theme file read (bytes).
pub const MAX_FILE: usize = 4 * 1024 * 1024;
/// The most templates and themes of the owner's listed.
pub const MAX_KEPT: usize = 200;
/// The longest name one has.
pub const MAX_NAME: usize = 60;

fn bad(m: &str) -> EditError {
    EditError::Invalid(m.to_string())
}

/// A name as it's kept: one clean line, at most [`MAX_NAME`] characters, else `or`.
pub fn clean_name(name: &str, or: &str) -> String {
    let s: String = name.chars().filter(|c| !c.is_control()).take(MAX_NAME).collect();
    let s = s.trim();
    if s.is_empty() { or.to_string() } else { s.to_string() }
}

/// Items of the layers kept to put on another recording: the layers they were on (named, picture
/// or sound, their volume), each item timed from the template's start, and the media bin's files
/// the items show (found where they were when it was made).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OwnTemplate {
    pub name: String,
    pub layers: Vec<Layer>,
    pub media: Vec<MediaItem>,
}

impl OwnTemplate {
    /// How long it is: to the end of its last item.
    pub fn length(&self) -> u64 {
        self.layers.iter().flat_map(|l| l.items.iter()).map(Item::end).max().unwrap_or(0)
    }

    pub fn item_count(&self) -> usize {
        self.layers.iter().map(|l| l.items.len()).sum()
    }

    /// Made safe, as a recording's layers are when they're read: too many dropped, numbers in
    /// range, items in order and apart, items of media it doesn't have dropped.
    pub fn checked(mut self) -> OwnTemplate {
        self.name = clean_name(&self.name, "Template");
        layers::check_parts(&mut self.media, &mut self.layers);
        self.layers.retain(|l| !l.items.is_empty());
        self
    }

    /// One read from its file (as the host kept it), made safe; none if it isn't one.
    pub fn from_json(text: &str) -> Option<OwnTemplate> {
        if text.len() > MAX_FILE {
            return None;
        }
        let t: OwnTemplate = serde_json::from_str(text).ok()?;
        let t = t.checked();
        (t.item_count() > 0).then_some(t)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// What a template put on the timeline: the layers made for it, and how many items went on them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Used {
    pub layers: Vec<u32>,
    pub items: usize,
}

impl Timeline {
    /// The layers' items (all of them, or those `chosen` as (layer, item)) as a template named
    /// `name`: timed from the first of them, with the files of the media bin they show.
    pub fn make_template(&self, name: &str, chosen: Option<&[(u32, u32)]>) -> Result<OwnTemplate, EditError> {
        let wanted = |l: &Layer, i: &Item| chosen.is_none_or(|c| c.contains(&(l.id, i.id)));
        let start = self.layers.iter().flat_map(|l| l.items.iter().filter(move |i| wanted(l, i))).map(|i| i.at).min();
        let Some(start) = start else {
            return Err(bad(if chosen.is_some() { "choose an item on a layer to keep" } else { "there's nothing on the layers to keep" }));
        };
        let mut kept = Vec::new();
        for l in &self.layers {
            let items: Vec<Item> = l.items.iter().filter(|i| wanted(l, i)).map(|i| Item { at: i.at - start, ..i.clone() }).collect();
            if !items.is_empty() {
                kept.push(Layer { items, ..l.clone() });
            }
        }
        let shown: Vec<u32> = kept
            .iter()
            .flat_map(|l| l.items.iter())
            .filter_map(|i| match i.what {
                What::Media { id } => Some(id),
                _ => None,
            })
            .collect();
        let media = self.media.iter().filter(|m| shown.contains(&m.id)).cloned().collect();
        Ok(OwnTemplate { name: clean_name(name, "Template"), layers: kept, media }.checked())
    }

    /// Puts a template's items on layers of their own from `at`, each where it was from the
    /// template's start (or the nearest place it fits); its files go into the media bin (one
    /// that's there already is used again).
    pub fn use_template(&mut self, t: &OwnTemplate, at: u64) -> Result<Used, EditError> {
        let t = t.clone().checked();
        if t.item_count() == 0 {
            return Err(bad("that template has nothing in it"));
        }
        let mut ids = std::collections::BTreeMap::new();
        for m in &t.media {
            ids.insert(m.id, self.add_media(MediaItem { id: 0, ..m.clone() })?);
        }
        let at = at.min(u64::MAX / 8);
        let mut used = Used::default();
        for l in &t.layers {
            let layer = self.add_layer(l.kind)?;
            // Its name, unless a layer here has it already.
            let name = crate::timeline::clean_name(&l.name).filter(|n| !self.layers.iter().any(|x| &x.name == n));
            if let Some(nl) = self.layers.iter_mut().find(|x| x.id == layer) {
                if let Some(n) = name {
                    nl.name = n;
                }
                nl.volume = l.volume;
                nl.mono = l.mono;
                nl.hidden = l.hidden;
            }
            for i in &l.items {
                let mut item = i.clone();
                if let What::Media { id } = &mut item.what {
                    match ids.get(id) {
                        Some(new) => *id = *new,
                        None => continue,
                    }
                }
                self.place(item, Some(layer), at.saturating_add(i.at))?;
                used.items += 1;
            }
            used.layers.push(layer);
        }
        Ok(used)
    }

    /// Every title, lower third, callout and graphic on the layers in a theme's colours and face;
    /// how many were changed.
    pub fn apply_theme(&mut self, theme: &VideoTheme) -> usize {
        let theme = theme.clone().checked();
        let mut n = 0;
        for l in &mut self.layers {
            for i in &mut l.items {
                if theme.restyle(i) {
                    n += 1;
                }
            }
        }
        n
    }
}

/// A theme: the colours of the titles, lower thirds, callouts and graphics, and the face of their
/// words.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VideoTheme {
    pub name: String,
    /// Bars, boxes, tags, lines, cards and transitions (`#rrggbb`).
    pub main: String,
    /// What points at something: highlights, circles, arrows, clicks.
    pub mark: String,
    /// A plain title's words.
    pub words: String,
    pub font: TextFont,
}

impl Default for VideoTheme {
    fn default() -> VideoTheme {
        theme("Theme", "#0b5fff", "#ff3b30", "#ffffff", TextFont::Sans)
    }
}

fn theme(name: &str, main: &str, mark: &str, words: &str, font: TextFont) -> VideoTheme {
    VideoTheme { name: name.to_string(), main: main.to_string(), mark: mark.to_string(), words: words.to_string(), font }
}

/// The themes that come with the program.
pub fn built_in() -> Vec<VideoTheme> {
    vec![
        theme("Clear Blue", "#0b5fff", "#ff3b30", "#ffffff", TextFont::Sans),
        theme("Ocean", "#0e7c86", "#ffb703", "#e8fbff", TextFont::Sans),
        theme("Sunset", "#e4572e", "#ffc914", "#fff3e6", TextFont::Slanted),
        theme("Forest", "#2d6a4f", "#f4a259", "#f1faee", TextFont::Sans),
        theme("Violet", "#6c4ab6", "#ffd166", "#ffffff", TextFont::Sans),
        theme("Typewriter", "#2b2b2b", "#d62828", "#f5f5f5", TextFont::Mono),
    ]
}

/// A colour as it's kept: `#rrggbb`, else `or`.
fn hex(text: &str, or: &str) -> String {
    let t = text.trim().trim_start_matches('#');
    if t.len() == 6 && t.chars().all(|c| c.is_ascii_hexdigit()) { format!("#{}", t.to_ascii_lowercase()) } else { or.to_string() }
}

impl VideoTheme {
    /// Every value as it should be (a hostile file's made sensible).
    pub fn checked(self) -> VideoTheme {
        VideoTheme {
            name: clean_name(&self.name, "Theme"),
            main: hex(&self.main, "#0b5fff"),
            mark: hex(&self.mark, "#ff3b30"),
            words: hex(&self.words, "#ffffff"),
            font: self.font,
        }
    }

    /// One read from its file (as the host kept it), made safe; none if it isn't one.
    pub fn from_json(text: &str) -> Option<VideoTheme> {
        if text.len() > MAX_FILE {
            return None;
        }
        serde_json::from_str::<VideoTheme>(text).ok().map(VideoTheme::checked)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// The colour it gives an item of a style, if it gives one: what points at something its
    /// mark, a plain title its words, the rest its main colour (a note stays a note's colour; a
    /// spotlight and a blur have none).
    pub fn colour_for(&self, style: TextStyle) -> Option<&str> {
        use TextStyle::*;
        match style {
            Spotlight | Blur | Note => None,
            Highlight | Circle | Arrow | Click => Some(&self.mark),
            Title => Some(&self.words),
            _ => Some(&self.main),
        }
    }

    /// An item in its colours and face (a title, a lower third, a callout, a graphic; anything
    /// else as it is): whether that changed it.
    pub fn restyle(&self, item: &mut Item) -> bool {
        let What::Text { style, color, font, .. } = &mut item.what else { return false };
        let before = (color.clone(), *font);
        if let Some(c) = self.colour_for(*style) {
            *color = c.to_string();
        }
        if style.has_words() {
            *font = self.font;
        }
        before != (color.clone(), *font)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::{LayerKind, MediaKind, template};
    use crate::recording::{Header, RecordingWriter, StreamInfo, StreamKind};

    fn recording(secs: u64) -> crate::recording::Recording {
        let streams = vec![StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 32, height: 18, channels: 0 }];
        let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
        let pic = screengrabcraft_model::Raster::filled(32, 18, screengrabcraft_model::Color::WHITE).unwrap();
        for t in 0..secs * 10 {
            w.video_frame(0, t * 100_000, &pic).unwrap();
        }
        let bytes = w.finish(secs * 1_000_000).unwrap();
        crate::recording::Recording::open(Box::new(bytes)).unwrap()
    }

    fn edited() -> Timeline {
        let mut tl = Timeline::new(&recording(20));
        let logo = tl
            .add_media(MediaItem {
                id: 0,
                name: "Logo".into(),
                path: "C:/Art/logo.png".into(),
                kind: MediaKind::Picture,
                duration: 0,
                width: 200,
                height: 100,
            })
            .unwrap();
        let (l1, _) = tl.place(template("lower-bar").unwrap().item, None, 2_000_000).unwrap();
        tl.place(template("title").unwrap().item, Some(l1), 8_000_000).unwrap();
        tl.place(Item { what: What::Media { id: logo }, ..Item::default() }, None, 3_000_000).unwrap();
        tl
    }

    #[test]
    fn the_layers_kept_as_a_template_go_on_another_recording_from_the_playhead() {
        let tl = edited();
        let t = tl.make_template("  My intro\n", None).unwrap();
        assert_eq!(t.name, "My intro");
        assert_eq!(t.item_count(), 3);
        assert_eq!(t.layers.len(), 2);
        // Timed from its first item, which starts it.
        assert_eq!(t.layers[0].items[0].at, 0);
        assert_eq!(t.layers[0].items[1].at, 6_000_000);
        assert_eq!(t.layers[1].items[0].at, 1_000_000);
        assert_eq!(t.media.len(), 1);
        // Through its file and back.
        let back = OwnTemplate::from_json(&t.to_json()).unwrap();
        assert_eq!(back, t);
        let mut other = Timeline::new(&recording(30));
        let used = other.use_template(&back, 5_000_000).unwrap();
        assert_eq!((used.layers.len(), used.items), (2, 3));
        let starts: Vec<u64> = other.layers.iter().flat_map(|l| l.items.iter().map(|i| i.at)).collect();
        assert_eq!(starts, vec![5_000_000, 11_000_000, 6_000_000]);
        assert_eq!(other.media.len(), 1);
        assert_eq!(other.media[0].path, "C:/Art/logo.png");
        // Its picture shows the file brought in with it.
        let id = other.media[0].id;
        assert!(other.layers[1].items[0].what == What::Media { id });
        // Again: the logo isn't brought in twice.
        other.use_template(&back, 15_000_000).unwrap();
        assert_eq!(other.media.len(), 1);
        assert_eq!(other.layers.len(), 4);
    }

    #[test]
    fn just_the_chosen_item_can_be_kept() {
        let tl = edited();
        let l = &tl.layers[0];
        let t = tl.make_template("Name card", Some(&[(l.id, l.items[0].id)])).unwrap();
        assert_eq!(t.item_count(), 1);
        assert!(t.media.is_empty());
        assert!(tl.make_template("x", Some(&[(999, 999)])).is_err());
        let empty = Timeline::new(&recording(2));
        assert!(empty.make_template("x", None).is_err());
    }

    #[test]
    fn a_hostile_template_or_theme_file_is_made_safe() {
        assert!(OwnTemplate::from_json("not json").is_none());
        assert!(OwnTemplate::from_json("{}").is_none(), "nothing in it");
        let long = "x".repeat(MAX_FILE + 1);
        assert!(OwnTemplate::from_json(&long).is_none());
        // An item of media it doesn't have is dropped; numbers are brought into range.
        let text = r##"{"name": "", "layers": [{"id": 0, "kind": "picture", "volume": 1e30, "items": [
            {"id": 3, "at": 18446744073709551615, "len": 0, "what": {"media": {"id": 77}}},
            {"id": 3, "at": 0, "len": 5, "place": {"x": 1e9, "w": -4}, "what": {"text": {"style": "title", "title": "Hi", "color": "#zzzzzzzzzzzz"}}}
        ]}]}"##;
        let t = OwnTemplate::from_json(text).unwrap();
        assert_eq!(t.name, "Template");
        assert_eq!(t.item_count(), 1);
        assert!(t.layers[0].volume <= 2.0);
        let mut tl = Timeline::new(&recording(5));
        tl.use_template(&t, 0).unwrap();
        let th = VideoTheme::from_json(r##"{"name": "\u0007", "main": "red", "mark": "#ABCDEF", "font": "mono"}"##).unwrap();
        assert_eq!((th.name.as_str(), th.main.as_str(), th.mark.as_str(), th.font), ("Theme", "#0b5fff", "#abcdef", TextFont::Mono));
        assert!(VideoTheme::from_json("5").is_none());
    }

    #[test]
    fn a_theme_colours_titles_lower_thirds_and_callouts_and_sets_their_face() {
        let mut tl = edited();
        tl.place(template("highlight").unwrap().item, None, 0).unwrap();
        tl.place(template("note").unwrap().item, None, 0).unwrap();
        let sunset = built_in().into_iter().find(|t| t.name == "Sunset").unwrap();
        // The lower third, the title, the highlight and the note (its face); not the picture.
        assert_eq!(tl.apply_theme(&sunset), 4);
        let texts: Vec<(TextStyle, String, TextFont)> = tl
            .layers
            .iter()
            .flat_map(|l| l.items.iter())
            .filter_map(|i| match &i.what {
                What::Text { style, color, font, .. } => Some((*style, color.clone(), *font)),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&(TextStyle::LowerBar, "#e4572e".into(), TextFont::Slanted)));
        assert!(texts.contains(&(TextStyle::Title, "#fff3e6".into(), TextFont::Slanted)));
        // A highlight has no words: just its colour.
        assert!(texts.contains(&(TextStyle::Highlight, "#ffc914".into(), TextFont::Sans)));
        assert!(texts.contains(&(TextStyle::Note, "#ffd84d".into(), TextFont::Slanted)));
        assert_eq!(tl.apply_theme(&sunset), 0, "already in it");
        // Every built-in theme has a name of its own and is as it should be.
        let all = built_in();
        for (i, t) in all.iter().enumerate() {
            assert_eq!(t.clone().checked(), *t);
            assert!(all.iter().skip(i + 1).all(|o| o.name != t.name));
        }
        let _ = LayerKind::Sound;
    }

    #[test]
    fn a_theme_s_face_draws_the_words_another_way() {
        let a =
            crate::stock::build_in(TextFont::Sans, TextStyle::LowerBar, "Alexandra Montgomery", "Engineer", [11, 95, 255], 5_000_000, 1.0).unwrap();
        let b =
            crate::stock::build_in(TextFont::Mono, TextStyle::LowerBar, "Alexandra Montgomery", "Engineer", [11, 95, 255], 5_000_000, 1.0).unwrap();
        let c = crate::stock::build_in(TextFont::Slanted, TextStyle::LowerBar, "Alexandra Montgomery", "Engineer", [11, 95, 255], 5_000_000, 1.0)
            .unwrap();
        let draw = |g: &crate::stock::Graphic| {
            let mut p = tiny_skia::Pixmap::new(480, 270).unwrap();
            g.animation.render_written(&mut p, 2.5, tiny_skia::Transform::from_scale(0.25, 0.25), 1.0);
            p.data().to_vec()
        };
        let (pa, pb, pc) = (draw(&a), draw(&b), draw(&c));
        assert_ne!(pa, pb);
        assert_ne!(pa, pc);
        // The fixed-width words are wider, and their plate with them.
        assert!(b.bounds.w > a.bounds.w, "{} {}", b.bounds.w, a.bounds.w);
    }
}
