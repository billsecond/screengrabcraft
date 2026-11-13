//! What's added to a recording in the video editor, over and beside its own tracks: the media
//! brought in (pictures, sound files, other recordings), and layers of items placed on the
//! timeline — media, titles and lower thirds, callouts and overlays, and transitions.
//!
//! A layer is a picture layer (drawn over the recording, the later layers on top) or a sound
//! layer (mixed in). Its items never overlap: an item put where another is goes to the nearest
//! place it fits, or onto a new layer.
//!
//! Times are in microseconds, like the rest of the timeline. Placements are shares of the video's
//! size (0 to 1), so they hold at any size the video is made at.

use serde::{Deserialize, Serialize};

use crate::timeline::{EditError, MIN_CLIP_US, Timeline};

/// The most layers and items a timeline keeps (more read from a file are dropped).
pub const MAX_LAYERS: usize = 64;
pub const MAX_ITEMS: usize = 2_000;
pub const MAX_MEDIA: usize = 500;
/// The longest text an item keeps.
pub const MAX_TEXT: usize = 400;

/// What a piece of media is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaKind {
    #[default]
    Picture,
    Sound,
    /// A video: another recording, or a video file.
    Video,
}

/// A file brought into the video editor (the media bin).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MediaItem {
    pub id: u32,
    pub name: String,
    /// Where it is, as the host found it.
    pub path: String,
    pub kind: MediaKind,
    /// How long it is (sound, video); 0 for a picture.
    pub duration: u64,
    /// Its picture's size (picture, video).
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LayerKind {
    /// Drawn over the recording's picture.
    #[default]
    Picture,
    /// Mixed into the sound.
    Sound,
}

/// A row of items on the timeline, apart and in order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Layer {
    pub id: u32,
    pub name: String,
    pub kind: LayerKind,
    /// Hidden (picture) or muted (sound).
    pub hidden: bool,
    /// 0 to 2 (sound).
    pub volume: f32,
    pub items: Vec<Item>,
}

impl Default for Layer {
    fn default() -> Layer {
        Layer { id: 0, name: String::new(), kind: LayerKind::Picture, hidden: false, volume: 1.0, items: Vec::new() }
    }
}

/// How an item comes in or goes out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnimKind {
    /// At once.
    None,
    #[default]
    Fade,
    /// From (or to) the left, the right, below.
    SlideLeft,
    SlideRight,
    SlideUp,
    /// Growing from (or shrinking to) nothing.
    Pop,
}

impl AnimKind {
    pub const ALL: [AnimKind; 6] = [AnimKind::None, AnimKind::Fade, AnimKind::SlideLeft, AnimKind::SlideRight, AnimKind::SlideUp, AnimKind::Pop];

    pub fn label(self) -> &'static str {
        match self {
            AnimKind::None => "None",
            AnimKind::Fade => "Fade",
            AnimKind::SlideLeft => "Slide from the left",
            AnimKind::SlideRight => "Slide from the right",
            AnimKind::SlideUp => "Slide up",
            AnimKind::Pop => "Pop",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Anim {
    pub kind: AnimKind,
    pub len: u64,
}

impl Default for Anim {
    fn default() -> Anim {
        Anim { kind: AnimKind::Fade, len: 400_000 }
    }
}

/// Where an item sits on the picture: its middle (shares of the video's width and height), its
/// width (a share of the video's; its height follows its own shape), and how see-through.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Placement {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub opacity: f32,
}

impl Default for Placement {
    fn default() -> Placement {
        Placement { x: 0.5, y: 0.5, w: 0.5, opacity: 1.0 }
    }
}

impl Placement {
    /// Every number in its range (a hostile file's NaNs made sensible).
    pub fn clamped(self) -> Placement {
        let f = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        Placement {
            x: f(self.x, -0.5, 1.5, 0.5),
            y: f(self.y, -0.5, 1.5, 0.5),
            w: f(self.w, 0.02, 3.0, 0.5),
            opacity: f(self.opacity, 0.0, 1.0, 1.0),
        }
    }
}

/// The look of a text item.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextStyle {
    /// Big words in the middle, a soft shadow behind.
    #[default]
    Title,
    /// Big words on a dark box.
    TitleBox,
    /// A name and a role on a coloured bar, at the bottom left.
    LowerBar,
    /// A name over a thin coloured line, a role under it.
    LowerLine,
    /// A name on a bright tag, a role on a dark one under it.
    LowerTag,
    /// Words in a rounded white box with a pointer.
    Callout,
    /// A yellow note.
    Note,
    /// A bar across the top with words on it.
    Banner,
    /// A coloured outline round a part of the picture (no words).
    Highlight,
    /// The picture darkened outside an oval (no words).
    Spotlight,
}

impl TextStyle {
    /// Whether it has words.
    pub fn has_words(self) -> bool {
        !matches!(self, TextStyle::Highlight | TextStyle::Spotlight)
    }

    /// Whether it has a second line.
    pub fn has_subtitle(self) -> bool {
        matches!(self, TextStyle::LowerBar | TextStyle::LowerLine | TextStyle::LowerTag)
    }
}

/// A screen transition: what it does to the picture over its time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransitionKind {
    /// From black.
    FadeIn,
    /// To black.
    FadeOut,
    /// Through black and back.
    #[default]
    DipBlack,
    /// Through white and back.
    DipWhite,
    /// A dark band sweeping across.
    Wipe,
    /// A bright flash.
    Flash,
}

/// What an item shows or plays.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum What {
    /// A picture, sound or video of the media bin.
    Media {
        id: u32,
    },
    Text {
        style: TextStyle,
        #[serde(default)]
        title: String,
        #[serde(default)]
        subtitle: String,
        /// Its main colour (the bar, the line, the tag), as `#rrggbb`.
        #[serde(default)]
        color: String,
    },
    Transition {
        kind: TransitionKind,
    },
}

impl Default for What {
    fn default() -> What {
        What::Transition { kind: TransitionKind::default() }
    }
}

/// Something placed on a layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Item {
    pub id: u32,
    pub at: u64,
    pub len: u64,
    /// Where it starts in its media (sound, video).
    pub from: u64,
    pub what: What,
    pub place: Placement,
    pub intro: Anim,
    pub outro: Anim,
}

impl Default for Item {
    fn default() -> Item {
        Item {
            id: 0,
            at: 0,
            len: 3_000_000,
            from: 0,
            what: What::default(),
            place: Placement::default(),
            intro: Anim::default(),
            outro: Anim::default(),
        }
    }
}

impl Item {
    pub fn end(&self) -> u64 {
        self.at.saturating_add(self.len)
    }

    /// The kind of layer it goes on.
    pub fn layer_kind(&self, media: &[MediaItem]) -> LayerKind {
        match &self.what {
            What::Media { id } if media.iter().any(|m| m.id == *id && m.kind == MediaKind::Sound) => LayerKind::Sound,
            _ => LayerKind::Picture,
        }
    }

    /// How far in it is at timeline time `t`: (how much it has come in, 0 to 1; how much it has
    /// gone out, 0 to 1).
    pub fn anim_at(&self, t: u64) -> (f32, f32) {
        let into = t.saturating_sub(self.at);
        let left = self.end().saturating_sub(t);
        let part = |done: u64, a: Anim| {
            if a.kind == AnimKind::None || a.len == 0 { 1.0 } else { (done as f32 / a.len.min(self.len / 2).max(1) as f32).clamp(0.0, 1.0) }
        };
        (part(into, self.intro), 1.0 - part(left, self.outro))
    }

    /// Its name on the timeline.
    pub fn label(&self, media: &[MediaItem]) -> String {
        match &self.what {
            What::Media { id } => media.iter().find(|m| m.id == *id).map(|m| m.name.clone()).unwrap_or_else(|| "Missing media".to_string()),
            What::Text { style, title, .. } if style.has_words() && !title.trim().is_empty() => title.clone(),
            What::Text { style, .. } => style_name(*style).to_string(),
            What::Transition { kind } => transition_name(*kind).to_string(),
        }
    }
}

pub fn style_name(s: TextStyle) -> &'static str {
    match s {
        TextStyle::Title => "Title",
        TextStyle::TitleBox => "Title on a box",
        TextStyle::LowerBar => "Lower third: bar",
        TextStyle::LowerLine => "Lower third: line",
        TextStyle::LowerTag => "Lower third: tags",
        TextStyle::Callout => "Callout",
        TextStyle::Note => "Note",
        TextStyle::Banner => "Banner",
        TextStyle::Highlight => "Highlight",
        TextStyle::Spotlight => "Spotlight",
    }
}

pub fn transition_name(k: TransitionKind) -> &'static str {
    match k {
        TransitionKind::FadeIn => "Fade in from black",
        TransitionKind::FadeOut => "Fade out to black",
        TransitionKind::DipBlack => "Dip to black",
        TransitionKind::DipWhite => "Dip to white",
        TransitionKind::Wipe => "Wipe",
        TransitionKind::Flash => "Flash",
    }
}

/// A group of the library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Titles,
    LowerThirds,
    Overlays,
    Transitions,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Titles, Group::LowerThirds, Group::Overlays, Group::Transitions];

    pub fn label(self) -> &'static str {
        match self {
            Group::Titles => "Titles",
            Group::LowerThirds => "Lower Thirds",
            Group::Overlays => "Callouts and Overlays",
            Group::Transitions => "Transitions",
        }
    }
}

/// A ready-made item of the library.
#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    /// Its name in commands (`lower-bar`).
    pub key: &'static str,
    pub name: &'static str,
    pub group: Group,
    pub item: Item,
}

fn text(style: TextStyle, title: &str, subtitle: &str, color: &str, place: Placement, len: u64, intro: AnimKind) -> Item {
    Item {
        len,
        what: What::Text { style, title: title.to_string(), subtitle: subtitle.to_string(), color: color.to_string() },
        place,
        intro: Anim { kind: intro, len: 450_000 },
        outro: Anim { kind: if intro == AnimKind::None { AnimKind::None } else { AnimKind::Fade }, len: 350_000 },
        ..Item::default()
    }
}

fn transition(kind: TransitionKind, len: u64) -> Item {
    Item {
        len,
        what: What::Transition { kind },
        intro: Anim { kind: AnimKind::None, len: 0 },
        outro: Anim { kind: AnimKind::None, len: 0 },
        ..Item::default()
    }
}

/// The library: titles, lower thirds, callouts and overlays, transitions. All drawn in code.
pub fn library() -> Vec<Template> {
    let p = |x: f32, y: f32, w: f32| Placement { x, y, w, opacity: 1.0 };
    let accent = "#6e5ad6";
    let t = |key, name, group, item| Template { key, name, group, item };
    vec![
        t("title", "Title", Group::Titles, text(TextStyle::Title, "Your title", "", "#ffffff", p(0.5, 0.45, 1.0), 4_000_000, AnimKind::Pop)),
        t(
            "title-box",
            "Title on a box",
            Group::Titles,
            text(TextStyle::TitleBox, "Your title", "", accent, p(0.5, 0.45, 1.0), 4_000_000, AnimKind::Fade),
        ),
        t("banner", "Banner", Group::Titles, text(TextStyle::Banner, "Chapter one", "", accent, p(0.5, 0.08, 1.0), 4_000_000, AnimKind::SlideUp)),
        t(
            "lower-bar",
            "Bar",
            Group::LowerThirds,
            text(TextStyle::LowerBar, "Your Name", "What you do", accent, p(0.3, 0.82, 1.0), 5_000_000, AnimKind::SlideLeft),
        ),
        t(
            "lower-line",
            "Line",
            Group::LowerThirds,
            text(TextStyle::LowerLine, "Your Name", "What you do", "#22c1a4", p(0.3, 0.82, 1.0), 5_000_000, AnimKind::Fade),
        ),
        t(
            "lower-tag",
            "Tags",
            Group::LowerThirds,
            text(TextStyle::LowerTag, "Your Name", "What you do", "#f5a524", p(0.3, 0.82, 1.0), 5_000_000, AnimKind::SlideLeft),
        ),
        t("callout", "Callout", Group::Overlays, text(TextStyle::Callout, "Click here", "", "#ffffff", p(0.62, 0.35, 1.0), 3_000_000, AnimKind::Pop)),
        t("note", "Note", Group::Overlays, text(TextStyle::Note, "Remember this", "", "#ffd84d", p(0.75, 0.25, 1.0), 3_000_000, AnimKind::Pop)),
        t("highlight", "Highlight", Group::Overlays, text(TextStyle::Highlight, "", "", "#ff3b30", p(0.5, 0.5, 0.3), 3_000_000, AnimKind::Fade)),
        t("spotlight", "Spotlight", Group::Overlays, text(TextStyle::Spotlight, "", "", "#000000", p(0.5, 0.5, 0.35), 3_000_000, AnimKind::Fade)),
        t("fade-in", "Fade in from black", Group::Transitions, transition(TransitionKind::FadeIn, 1_000_000)),
        t("fade-out", "Fade out to black", Group::Transitions, transition(TransitionKind::FadeOut, 1_000_000)),
        t("dip-black", "Dip to black", Group::Transitions, transition(TransitionKind::DipBlack, 1_000_000)),
        t("dip-white", "Dip to white", Group::Transitions, transition(TransitionKind::DipWhite, 1_000_000)),
        t("wipe", "Wipe", Group::Transitions, transition(TransitionKind::Wipe, 900_000)),
        t("flash", "Flash", Group::Transitions, transition(TransitionKind::Flash, 600_000)),
    ]
}

/// A ready-made item by its key.
pub fn template(key: &str) -> Option<Template> {
    library().into_iter().find(|t| t.key == key)
}

/// The colour of `#rrggbb`, or `or`.
pub fn color(text: &str, or: [u8; 3]) -> [u8; 3] {
    let h = text.trim().trim_start_matches('#');
    if h.len() != 6 || !h.is_ascii() {
        return or;
    }
    let byte = |i: usize| h.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok());
    match (byte(0), byte(2), byte(4)) {
        (Some(r), Some(g), Some(b)) => [r, g, b],
        _ => or,
    }
}

/// Where an item fits on a layer of items: `at` if it's free, else the free start nearest to it
/// (never before 0). `skip` is an item not to count (the one being moved).
pub fn free_start(items: &[Item], at: u64, len: u64, skip: Option<u32>) -> Option<u64> {
    let len = len.max(1);
    let mut busy: Vec<(u64, u64)> = items.iter().filter(|i| Some(i.id) != skip).map(|i| (i.at, i.end())).collect();
    busy.sort_unstable();
    let fits = |s: u64| busy.iter().all(|&(a, b)| s.saturating_add(len) <= a || s >= b);
    if fits(at) {
        return Some(at);
    }
    // Every start next to an item: just after it, or just before it.
    let mut candidates: Vec<u64> = busy.iter().flat_map(|&(a, b)| [b, a.saturating_sub(len)]).filter(|s| fits(*s)).collect();
    candidates.sort_by_key(|s| s.abs_diff(at));
    candidates.first().copied()
}

fn bad(m: &str) -> EditError {
    EditError::Invalid(m.to_string())
}

impl Timeline {
    /// A number not yet used by a layer, an item or a piece of media.
    fn next_id(&self) -> u32 {
        let layers = self.layers.iter().map(|l| l.id);
        let items = self.layers.iter().flat_map(|l| l.items.iter().map(|i| i.id));
        let media = self.media.iter().map(|m| m.id);
        layers.chain(items).chain(media).max().unwrap_or(0).saturating_add(1)
    }

    pub fn layer(&self, id: u32) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    fn layer_mut(&mut self, id: u32) -> Result<&mut Layer, EditError> {
        self.layers.iter_mut().find(|l| l.id == id).ok_or_else(|| bad("there's no such layer"))
    }

    /// The layer and item of an item's id.
    pub fn item(&self, layer: u32, item: u32) -> Option<&Item> {
        self.layer(layer)?.items.iter().find(|i| i.id == item)
    }

    /// Brings a file into the media bin; its id.
    pub fn add_media(&mut self, mut m: MediaItem) -> Result<u32, EditError> {
        if self.media.len() >= MAX_MEDIA {
            return Err(bad("the media bin is full"));
        }
        if let Some(same) = self.media.iter().find(|x| x.path == m.path && !m.path.is_empty()) {
            return Ok(same.id);
        }
        m.id = self.next_id();
        m.name = clip_text(&m.name, 120);
        self.media.push(m.clone());
        Ok(m.id)
    }

    /// Takes a file out of the media bin, and every item that showed it.
    pub fn remove_media(&mut self, id: u32) -> Result<(), EditError> {
        let before = self.media.len();
        self.media.retain(|m| m.id != id);
        if self.media.len() == before {
            return Err(bad("there's no such media"));
        }
        for l in &mut self.layers {
            l.items.retain(|i| i.what != What::Media { id });
        }
        Ok(())
    }

    /// A new, empty layer of a kind, on top of the others; its id.
    pub fn add_layer(&mut self, kind: LayerKind) -> Result<u32, EditError> {
        if self.layers.len() >= MAX_LAYERS {
            return Err(bad("there are as many layers as there can be"));
        }
        let id = self.next_id();
        let n = self.layers.iter().filter(|l| l.kind == kind).count() + 1;
        let name = match kind {
            LayerKind::Picture => format!("Layer {n}"),
            LayerKind::Sound => format!("Sound {n}"),
        };
        self.layers.push(Layer { id, name, kind, ..Layer::default() });
        Ok(id)
    }

    /// Puts an item on a layer (`None`: a new one) at `at`, or the nearest place it fits there;
    /// where it doesn't fit at all, or the layer is of the other kind, on a new layer. Its media
    /// must be in the bin. The layer and item it went to.
    pub fn place(&mut self, mut item: Item, layer: Option<u32>, at: u64) -> Result<(u32, u32), EditError> {
        if self.layers.iter().map(|l| l.items.len()).sum::<usize>() >= MAX_ITEMS {
            return Err(bad("there are as many items as there can be"));
        }
        let kind = item.layer_kind(&self.media);
        if let What::Media { id } = &item.what {
            let m = self.media.iter().find(|m| m.id == *id).ok_or_else(|| bad("that media isn't in the bin"))?;
            if m.kind != MediaKind::Picture && m.duration > 0 {
                let most = m.duration.saturating_sub(item.from);
                if most < MIN_CLIP_US {
                    return Err(bad("there's nothing left of that media to place"));
                }
                item.len = item.len.min(most);
            }
        }
        item.len = item.len.clamp(MIN_CLIP_US, u64::MAX / 8);
        item.place = item.place.clamped();
        let at = at.min(u64::MAX / 8);
        let spot =
            layer.and_then(|id| self.layer(id)).filter(|l| l.kind == kind).and_then(|l| free_start(&l.items, at, item.len, None).map(|s| (l.id, s)));
        let (layer, start) = match spot {
            Some(s) => s,
            None => (self.add_layer(kind)?, at),
        };
        item.id = self.next_id();
        item.at = start;
        let id = item.id;
        let l = self.layer_mut(layer)?;
        l.items.push(item);
        l.items.sort_by_key(|i| i.at);
        Ok((layer, id))
    }

    /// Moves an item to `at` on a layer (`None`: a new one), or the nearest place it fits there.
    pub fn move_item(&mut self, layer: u32, item: u32, to_layer: Option<u32>, at: u64) -> Result<(u32, u32), EditError> {
        let current = self.item(layer, item).cloned().ok_or_else(|| bad("there's no such item"))?;
        // Staying on its layer: the nearest free place there, itself not counted.
        if to_layer == Some(layer) {
            let l = self.layer_mut(layer)?;
            let start = free_start(&l.items, at.min(u64::MAX / 8), current.len, Some(item)).unwrap_or(current.at);
            if let Some(i) = l.items.iter_mut().find(|i| i.id == item) {
                i.at = start;
            }
            l.items.sort_by_key(|i| i.at);
            return Ok((layer, item));
        }
        // Onto another layer: only one of its kind (a picture never goes onto a sound layer).
        if let Some(to) = to_layer {
            let kind = current.layer_kind(&self.media);
            if self.layer(to).map(|l| l.kind) != Some(kind) {
                return Err(bad("it can't go on that layer"));
            }
        }
        self.layer_mut(layer)?.items.retain(|i| i.id != item);
        let (to, _) = self.place(current, to_layer, at)?;
        // It keeps its number.
        if let Some(l) = self.layers.iter_mut().find(|l| l.id == to)
            && let Some(last) = l.items.iter_mut().max_by_key(|i| i.id)
        {
            last.id = item;
        }
        Ok((to, item))
    }

    /// Moves an item's start and end (timeline times), between its neighbours and inside its
    /// media.
    pub fn trim_item(&mut self, layer: u32, item: u32, start: u64, end: u64) -> Result<(), EditError> {
        let media = self.media.clone();
        let l = self.layer_mut(layer)?;
        let k = l.items.iter().position(|i| i.id == item).ok_or_else(|| bad("there's no such item"))?;
        let floor = k.checked_sub(1).and_then(|p| l.items.get(p)).map(Item::end).unwrap_or(0);
        let ceiling = l.items.get(k + 1).map(|n| n.at).unwrap_or(u64::MAX / 8);
        let Some(i) = l.items.get_mut(k) else { return Err(bad("there's no such item")) };
        let source = match &i.what {
            What::Media { id } => media.iter().find(|m| m.id == *id).filter(|m| m.kind != MediaKind::Picture).map(|m| m.duration),
            _ => None,
        };
        // A media item's start can't go back past its media's start.
        let floor = match source {
            Some(_) => floor.max(i.at.saturating_sub(i.from)),
            None => floor,
        };
        let start = start.clamp(floor, i.end().saturating_sub(MIN_CLIP_US).max(floor));
        let from = if source.is_some() { (i.from + start).saturating_sub(i.at) } else { 0 };
        let most_end = match source {
            Some(d) => start.saturating_add(d.saturating_sub(from)),
            None => u64::MAX / 8,
        };
        let end = end.clamp(start + MIN_CLIP_US, ceiling.min(most_end).max(start + MIN_CLIP_US));
        *i = Item { at: start, len: end - start, from, ..i.clone() };
        Ok(())
    }

    pub fn delete_item(&mut self, layer: u32, item: u32) -> Result<(), EditError> {
        let l = self.layer_mut(layer)?;
        let before = l.items.len();
        l.items.retain(|i| i.id != item);
        if l.items.len() == before { Err(bad("there's no such item")) } else { Ok(()) }
    }

    pub fn delete_layer(&mut self, layer: u32) -> Result<(), EditError> {
        let before = self.layers.len();
        self.layers.retain(|l| l.id != layer);
        if self.layers.len() == before { Err(bad("there's no such layer")) } else { Ok(()) }
    }

    /// Changes an item in place (its words, its look, where it sits, how it comes and goes).
    pub fn set_item(&mut self, layer: u32, item: u32, change: impl FnOnce(&mut Item)) -> Result<(), EditError> {
        let l = self.layer_mut(layer)?;
        let i = l.items.iter_mut().find(|i| i.id == item).ok_or_else(|| bad("there's no such item"))?;
        let (id, at, len, from) = (i.id, i.at, i.len, i.from);
        change(i);
        // Its place on the timeline is the trims' and moves' to change.
        i.id = id;
        (i.at, i.len, i.from) = (at, len, from);
        i.place = i.place.clamped();
        if let What::Text { title, subtitle, color, .. } = &mut i.what {
            *title = clip_text(title, MAX_TEXT);
            *subtitle = clip_text(subtitle, MAX_TEXT);
            *color = clip_text(color, 9);
        }
        Ok(())
    }

    pub fn set_layer(&mut self, layer: u32, hidden: Option<bool>, volume: Option<f32>, name: Option<&str>) -> Result<(), EditError> {
        let l = self.layer_mut(layer)?;
        if let Some(h) = hidden {
            l.hidden = h;
        }
        if let Some(v) = volume {
            if !v.is_finite() {
                return Err(bad("the volume is not a number"));
            }
            l.volume = v.clamp(0.0, 2.0);
        }
        if let Some(n) = name {
            l.name = clip_text(n, 60);
        }
        Ok(())
    }

    /// Takes `from..to` out of every layer too (as a cut does the recording's tracks).
    pub(crate) fn cut_layers(&mut self, from: u64, to: u64) {
        let gap = to - from;
        let media = self.media.clone();
        for l in &mut self.layers {
            let mut out = Vec::with_capacity(l.items.len() + 1);
            for i in &l.items {
                let plays = matches!(&i.what, What::Media { id } if media.iter().any(|m| m.id == *id && m.kind != MediaKind::Picture));
                if i.at < from {
                    out.push(Item { len: i.len.min(from - i.at), ..i.clone() });
                }
                if i.end() > to {
                    let skip = to.saturating_sub(i.at);
                    let from_media = if plays { i.from + skip } else { i.from };
                    let mut after = Item { at: i.at.max(to) - gap, from: from_media, len: i.len - skip, ..i.clone() };
                    // Both halves of one item: the second gets a number of its own below.
                    if i.at < from {
                        after.id = 0;
                    }
                    out.push(after);
                }
            }
            out.retain(|i| i.len >= MIN_CLIP_US);
            l.items = out;
        }
        // Halves of items cut in two get numbers of their own.
        let mut next = self.next_id();
        for l in &mut self.layers {
            for i in &mut l.items {
                if i.id == 0 {
                    i.id = next;
                    next = next.saturating_add(1);
                }
            }
        }
    }

    /// The end of the last item of any layer.
    pub(crate) fn layers_length(&self) -> u64 {
        self.layers.iter().flat_map(|l| l.items.iter()).map(Item::end).max().unwrap_or(0)
    }

    /// Layers, items and media read from a file, made safe: too many dropped, numbers in range,
    /// items in order and apart, items of media that isn't there dropped, ids unique.
    pub(crate) fn check_layers(&mut self) {
        self.media.truncate(MAX_MEDIA);
        let mut seen = std::collections::BTreeSet::new();
        self.media.retain(|m| seen.insert(m.id));
        for m in &mut self.media {
            m.name = clip_text(&m.name, 120);
            m.duration = m.duration.min(u64::MAX / 8);
        }
        let media = self.media.clone();
        self.layers.truncate(MAX_LAYERS);
        let mut items = 0usize;
        for l in &mut self.layers {
            l.volume = if l.volume.is_finite() { l.volume.clamp(0.0, 2.0) } else { 1.0 };
            l.name = clip_text(&l.name, 60);
            l.items.retain(|i| match &i.what {
                What::Media { id } => media.iter().any(|m| m.id == *id),
                _ => true,
            });
            for i in &mut l.items {
                i.at = i.at.min(u64::MAX / 8);
                i.len = i.len.clamp(MIN_CLIP_US, u64::MAX / 8);
                i.from = i.from.min(u64::MAX / 8);
                i.place = i.place.clamped();
                i.intro.len = i.intro.len.min(10_000_000);
                i.outro.len = i.outro.len.min(10_000_000);
                if let What::Text { title, subtitle, color, .. } = &mut i.what {
                    *title = clip_text(title, MAX_TEXT);
                    *subtitle = clip_text(subtitle, MAX_TEXT);
                    *color = clip_text(color, 9);
                }
            }
            l.items.sort_by_key(|i| i.at);
            let mut kept: Vec<Item> = Vec::with_capacity(l.items.len());
            for i in std::mem::take(&mut l.items) {
                if items < MAX_ITEMS && kept.last().is_none_or(|p| p.end() <= i.at) {
                    items += 1;
                    kept.push(i);
                }
            }
            l.items = kept;
        }
        // Every layer and item a number of its own.
        let mut used = std::collections::BTreeSet::new();
        for m in &self.media {
            used.insert(m.id);
        }
        let mut next = used.iter().max().copied().unwrap_or(0).saturating_add(1);
        let mut fresh = |id: &mut u32, used: &mut std::collections::BTreeSet<u32>| {
            if *id == 0 || !used.insert(*id) {
                while used.contains(&next) {
                    next = next.saturating_add(1);
                }
                *id = next;
                used.insert(next);
            }
        };
        for l in &mut self.layers {
            fresh(&mut l.id, &mut used);
            for i in &mut l.items {
                fresh(&mut i.id, &mut used);
            }
        }
    }
}

/// `s` no longer than `most` characters.
fn clip_text(s: &str, most: usize) -> String {
    s.chars().take(most).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::StreamKind;
    use crate::timeline::{CameraLayout, CaptionOptions, Clip, Track};

    fn timeline() -> Timeline {
        Timeline {
            tracks: vec![Track {
                stream: 0,
                kind: StreamKind::Screen,
                muted: false,
                volume: 1.0,
                clips: vec![Clip { at: 0, from: 0, len: 20_000_000 }],
            }],
            camera: CameraLayout::default(),
            captions: CaptionOptions::default(),
            layers: Vec::new(),
            media: Vec::new(),
        }
    }

    fn lower() -> Item {
        template("lower-bar").unwrap().item
    }

    #[test]
    fn items_never_overlap_they_go_to_the_nearest_gap_or_a_new_layer() {
        let mut tl = timeline();
        let (a, first) = tl.place(lower(), None, 1_000_000).unwrap();
        assert_eq!(tl.layers.len(), 1);
        // Dropped on top of the first: the nearest place it fits on that layer, after it.
        let (b, second) = tl.place(lower(), Some(a), 2_000_000).unwrap();
        assert_eq!(a, b);
        let l = tl.layer(a).unwrap();
        assert_eq!(l.items.len(), 2);
        assert_eq!(tl.item(a, second).unwrap().at, tl.item(a, first).unwrap().end());
        assert!(l.items.windows(2).all(|w| w[0].end() <= w[1].at));
        // A transition on a sound layer doesn't go there: a picture layer of its own.
        let sound = tl.add_layer(LayerKind::Sound).unwrap();
        let (c, _) = tl.place(template("dip-black").unwrap().item, Some(sound), 0).unwrap();
        assert_ne!(c, sound);
        assert_eq!(tl.layer(c).unwrap().kind, LayerKind::Picture);
        // Moving onto another item: the nearest free place, never on top of it.
        tl.move_item(a, second, Some(a), 1_500_000).unwrap();
        let l = tl.layer(a).unwrap();
        assert!(l.items.windows(2).all(|w| w[0].end() <= w[1].at), "{:?}", l.items);
        // To a new layer.
        let (d, kept) = tl.move_item(a, second, None, 500_000).unwrap();
        assert_eq!(kept, second);
        assert_ne!(d, a);
        assert_eq!(tl.item(d, second).unwrap().at, 500_000);
        assert_eq!(tl.layers_length(), tl.layers.iter().flat_map(|l| &l.items).map(Item::end).max().unwrap());
    }

    #[test]
    fn media_is_brought_in_once_and_its_items_keep_inside_it() {
        let mut tl = timeline();
        let song = MediaItem {
            name: "Song.mp3".into(),
            path: "C:/Music/Song.mp3".into(),
            kind: MediaKind::Sound,
            duration: 5_000_000,
            ..MediaItem::default()
        };
        let id = tl.add_media(song.clone()).unwrap();
        assert_eq!(tl.add_media(song).unwrap(), id, "the same file once");
        let (layer, item) = tl.place(Item { what: What::Media { id }, len: 60_000_000, ..Item::default() }, None, 0).unwrap();
        assert_eq!(tl.layer(layer).unwrap().kind, LayerKind::Sound);
        assert_eq!(tl.item(layer, item).unwrap().len, 5_000_000, "no longer than the song");
        // Trimmed: the start can't go before the song's, the end past its end.
        tl.trim_item(layer, item, 1_000_000, 9_000_000).unwrap();
        let i = tl.item(layer, item).unwrap();
        assert_eq!((i.at, i.from, i.end()), (1_000_000, 1_000_000, 5_000_000));
        tl.trim_item(layer, item, 0, 2_000_000).unwrap();
        assert_eq!((tl.item(layer, item).unwrap().at, tl.item(layer, item).unwrap().from), (0, 0));
        // Taken out of the bin: its items go too.
        tl.remove_media(id).unwrap();
        assert!(tl.layer(layer).unwrap().items.is_empty());
        assert!(tl.place(Item { what: What::Media { id }, ..Item::default() }, None, 0).is_err());
    }

    #[test]
    fn a_cut_takes_the_stretch_out_of_the_layers_too() {
        let mut tl = timeline();
        let (l, i) = tl.place(lower(), None, 2_000_000).unwrap();
        tl.cut(3_000_000, 4_000_000).unwrap();
        let items = &tl.layer(l).unwrap().items;
        assert_eq!(items.len(), 2, "cut in two");
        assert_eq!((items[0].at, items[0].len), (2_000_000, 1_000_000));
        assert_eq!((items[1].at, items[1].len), (3_000_000, 3_000_000));
        assert_eq!(items[0].id, i);
        assert_ne!(items[1].id, i, "the second half has a number of its own");
    }

    #[test]
    fn junk_from_a_file_is_made_safe() {
        let mut tl = timeline();
        let item = Item { id: 7, at: 5, len: 0, place: Placement { x: f32::NAN, y: f32::INFINITY, w: -4.0, opacity: 9.0 }, ..Item::default() };
        tl.layers = vec![
            Layer { id: 7, volume: f32::NAN, items: vec![item.clone(), Item { id: 7, at: 6, ..item.clone() }], ..Layer::default() },
            Layer { id: 7, items: vec![Item { what: What::Media { id: 99 }, ..Item::default() }], ..Layer::default() },
        ];
        tl.check_layers();
        let checked = tl;
        let l = &checked.layers[0];
        assert_eq!(l.volume, 1.0);
        assert_eq!(l.items.len(), 1, "overlapping items: the first kept");
        let p = l.items[0].place;
        assert!(p.x.is_finite() && p.y.is_finite() && p.w > 0.0 && p.opacity <= 1.0);
        assert!(checked.layers[1].items.is_empty(), "media that isn't there");
        let mut ids: Vec<u32> = checked.layers.iter().map(|l| l.id).chain(checked.layers.iter().flat_map(|l| l.items.iter().map(|i| i.id))).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "every number once");
    }

    #[test]
    fn the_library_has_each_kind_and_its_keys_are_unique() {
        let lib = library();
        for g in Group::ALL {
            assert!(lib.iter().filter(|t| t.group == g).count() >= 3, "{g:?}");
        }
        let mut keys: Vec<&str> = lib.iter().map(|t| t.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), lib.len());
        assert_eq!(color("#6e5ad6", [0, 0, 0]), [0x6e, 0x5a, 0xd6]);
        assert_eq!(color("nonsense", [1, 2, 3]), [1, 2, 3]);
        assert_eq!(color("#ééé", [1, 2, 3]), [1, 2, 3]);
    }

    #[test]
    fn an_item_comes_in_and_goes_out_over_its_animations() {
        let i = Item {
            at: 1_000_000,
            len: 4_000_000,
            intro: Anim { kind: AnimKind::Fade, len: 1_000_000 },
            outro: Anim { kind: AnimKind::Fade, len: 1_000_000 },
            ..Item::default()
        };
        assert_eq!(i.anim_at(1_000_000), (0.0, 0.0));
        assert_eq!(i.anim_at(1_500_000).0, 0.5);
        assert_eq!(i.anim_at(3_000_000), (1.0, 0.0));
        assert_eq!(i.anim_at(4_500_000).1, 0.5);
        let still = Item { intro: Anim { kind: AnimKind::None, len: 0 }, ..i };
        assert_eq!(still.anim_at(1_000_000).0, 1.0);
    }
}
