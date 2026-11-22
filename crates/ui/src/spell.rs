//! Spelling, checked while a text or a callout is typed: the words the English word list doesn't
//! know get a red wavy line under them, and a right click on one offers the list's suggestions,
//! Add to Dictionary (the word kept in the editor's settings) and the setting that turns checking
//! off (Edit › Check Spelling has it too).
//!
//! The checker is `spellbook` (MPL-2.0, reads Hunspell dictionaries); the word list is SCOWL's
//! en_US Hunspell dictionary as released (`assets/dictionaries`, its notices in
//! `README_en_US.txt`, shown in About), built into the program. Reading it takes a moment, so it
//! is read once, on a thread of its own where there are threads (in the browser, at once): words
//! are underlined once it is ready. Typed words are hostile like anything else from outside: the
//! longest are let be, and a checker that panics counts as knowing the word.

use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::text::CCursor;
use egui::{Color32, Pos2, Rect, Stroke, pos2};
use screengrabcraft_model::ObjectId;
use spellbook::Dictionary;

const AFF: &str = include_str!("../../../assets/dictionaries/en_US.aff");
const DIC: &str = include_str!("../../../assets/dictionaries/en_US.dic");
/// The word list's copyright notices and licences, as released with it (About shows them).
pub const NOTICE: &str = include_str!("../../../assets/dictionaries/README_en_US.txt");

static WORDS: OnceLock<Option<Dictionary>> = OnceLock::new();
static STARTED: AtomicBool = AtomicBool::new(false);

/// The longest word checked, and the longest given suggestions, in characters.
pub const MAX_WORD: usize = 48;
const MAX_SUGGESTED: usize = 32;
/// The most words of one text checked.
const MAX_WORDS: usize = 5_000;
/// The most suggestions offered.
pub const MAX_SUGGESTIONS: usize = 6;

/// What the menu's items are called.
pub const ADD_WORD: &str = "Add to Dictionary";
pub const CHECK_SPELLING: &str = "Check Spelling";
pub const NO_SUGGESTIONS: &str = "No suggestions";

fn read() -> Option<Dictionary> {
    catch_unwind(|| Dictionary::new(AFF, DIC).ok()).ok().flatten()
}

/// Starts reading the word list if it isn't read yet: on a thread of its own, or here and now
/// where there are no threads.
pub fn prepare() {
    if WORDS.get().is_some() || STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    let spawned = std::thread::Builder::new().name("spelling".to_string()).spawn(|| {
        let _ = WORDS.get_or_init(read);
    });
    if spawned.is_err() {
        let _ = WORDS.get_or_init(read);
    }
}

/// The word list, once it is read.
fn words() -> Option<&'static Dictionary> {
    WORDS.get().and_then(Option::as_ref)
}

/// The word list read here and now if it isn't yet; whether it could be.
pub fn ready_now() -> bool {
    STARTED.store(true, Ordering::Relaxed);
    WORDS.get_or_init(read).is_some()
}

fn is_apostrophe(c: char) -> bool {
    c == '\'' || c == '\u{2019}'
}

/// The words of `text` worth checking, each with where it is in characters: the letters of each
/// run between spaces, without the punctuation round them, split at hyphens. A run with digits or
/// other signs in it (a number, an address, a file name) is no word to check, nor is one of
/// capitals only (an abbreviation).
pub fn words_in(text: &str) -> Vec<(Range<usize>, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for i in 0..=chars.len() {
        let space = chars.get(i).is_none_or(|c| c.is_whitespace());
        match (space, start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                start = None;
                chunk(&chars, s..i, &mut out);
                if out.len() >= MAX_WORDS {
                    out.truncate(MAX_WORDS);
                    break;
                }
            }
            _ => {}
        }
    }
    out
}

/// The words of one run of `chars` between spaces.
fn chunk(chars: &[char], run: Range<usize>, out: &mut Vec<(Range<usize>, String)>) {
    let Some(part) = chars.get(run.clone()) else { return };
    // The punctuation round it (quotes, brackets, a full stop) isn't part of it.
    let Some(first) = part.iter().position(|c| c.is_alphanumeric()) else { return };
    let Some(last) = part.iter().rposition(|c| c.is_alphanumeric()) else { return };
    let Some(core) = part.get(first..=last) else { return };
    if !core.iter().all(|c| c.is_alphabetic() || is_apostrophe(*c) || *c == '-') {
        return;
    }
    let mut at = run.start + first;
    for piece in core.split(|c| *c == '-') {
        let begin = at;
        at += piece.len() + 1;
        let (Some(a), Some(b)) = (piece.iter().position(|c| c.is_alphabetic()), piece.iter().rposition(|c| c.is_alphabetic())) else {
            continue;
        };
        let Some(word) = piece.get(a..=b) else { continue };
        let letters = word.iter().filter(|c| c.is_alphabetic()).count();
        if letters < 2 || word.iter().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase()) {
            continue;
        }
        out.push((begin + a..begin + b + 1, word.iter().collect()));
    }
}

/// Whether `word` is in the word list or the owner's words.
fn known(d: &Dictionary, word: &str, own: &[String]) -> bool {
    if word.chars().count() > MAX_WORD || own.iter().any(|o| o.to_lowercase() == word.to_lowercase()) {
        return true;
    }
    catch_unwind(AssertUnwindSafe(|| d.check(word))).unwrap_or(true)
}

/// A word the word list doesn't know: where it is in its text, in characters, and the word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Misspelt {
    pub chars: Range<usize>,
    pub word: String,
}

/// The words of `text` that neither the word list nor `own` (the owner's words) knows. None
/// until the word list is read.
pub fn misspelt(text: &str, own: &[String]) -> Vec<Misspelt> {
    let Some(d) = words() else { return Vec::new() };
    words_in(text).into_iter().filter(|(_, w)| !known(d, w, own)).map(|(chars, word)| Misspelt { chars, word }).collect()
}

/// What the word list offers in place of `word`, best first.
pub fn suggestions(word: &str) -> Vec<String> {
    let Some(d) = words() else { return Vec::new() };
    if word.chars().count() > MAX_SUGGESTED {
        return Vec::new();
    }
    let offered = |w: &str| {
        let mut out = Vec::new();
        if catch_unwind(AssertUnwindSafe(|| d.suggest(w, &mut out))).is_err() {
            out.clear();
        }
        out
    };
    let mut all = Vec::new();
    // A word with a capital first letter only: the lower-case word's suggestions first, given the
    // capital (the checker otherwise favours short words that keep the capital where it is).
    let mut letters = word.chars();
    if letters.next().is_some_and(char::is_uppercase) && !letters.any(char::is_uppercase) {
        all.extend(offered(&word.to_lowercase()).iter().map(|s| capitalised(s)));
    }
    all.extend(offered(word));
    // Each once, one word before two.
    let mut out: Vec<String> = Vec::new();
    for s in all {
        if !s.is_empty() && s.chars().count() <= MAX_WORD && !out.contains(&s) {
            out.push(s);
        }
    }
    out.sort_by_key(|s| s.contains(' '));
    out.truncate(MAX_SUGGESTIONS);
    out
}

/// `word` with its first letter a capital.
fn capitalised(word: &str) -> String {
    let mut c = word.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// `text` with the characters in `chars` replaced by `with`; where the cursor goes after it.
pub fn replaced(text: &str, chars: &Range<usize>, with: &str) -> (String, usize) {
    let mut out: String = text.chars().take(chars.start).collect();
    out.push_str(with);
    let cursor = out.chars().count();
    out.extend(text.chars().skip(chars.end));
    (out, cursor)
}

/// The right-click menu on a text being typed.
#[derive(Clone, Debug)]
pub struct Menu {
    /// The annotation typed into.
    pub id: ObjectId,
    /// Where it was opened, on the screen.
    pub at: Pos2,
    /// The word under the pointer the word list doesn't know, and what it offers in its place.
    pub word: Option<(Misspelt, Vec<String>)>,
}

/// What the editor keeps for this: the last text checked and its unknown words, the menu.
#[derive(Clone, Debug, Default)]
pub struct State {
    checked: Option<(String, usize, Vec<Misspelt>)>,
    pub menu: Option<Menu>,
    /// The unknown words of the text being typed, as last underlined, each with where its line
    /// is on the screen (tests find them by it).
    pub shown: Vec<(Misspelt, Rect)>,
}

impl State {
    /// The unknown words of `text`, checked again only when it or the owner's words changed.
    pub fn of(&mut self, text: &str, own: &[String]) -> Vec<Misspelt> {
        if let Some((t, n, found)) = &self.checked
            && t == text
            && *n == own.len()
            && words().is_some()
        {
            return found.clone();
        }
        let found = misspelt(text, own);
        if words().is_some() {
            self.checked = Some((text.to_string(), own.len(), found.clone()));
        }
        found
    }
}

/// A word's rectangle on each row of the text box it is on, on the screen.
fn rows(galley: &egui::Galley, origin: Pos2, chars: &Range<usize>) -> Vec<Rect> {
    let mut rows: Vec<Rect> = Vec::new();
    for i in chars.clone().take(MAX_WORD) {
        let a = galley.pos_from_cursor(CCursor::new(i));
        let b = galley.pos_from_cursor(CCursor::new(i + 1));
        if (a.min.y - b.min.y).abs() > 0.5 {
            // The letter ends its row.
            continue;
        }
        let r = Rect::from_min_max(pos2(a.min.x, a.min.y), pos2(b.min.x, a.max.y)).translate(origin.to_vec2());
        match rows.last_mut() {
            Some(last) if (last.min.y - r.min.y).abs() < 0.5 => *last = last.union(r),
            _ => rows.push(r),
        }
    }
    rows
}

/// Puts the text box's cursor at character `at`.
fn set_cursor(ctx: &egui::Context, id: egui::Id, at: usize) {
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(CCursor::new(at))));
        state.store(ctx, id);
    }
}

/// While annotation `id` is typed into (its text box `shown`, its text `text`): the words the
/// word list doesn't know underlined, the menu a right click on the box opens, and what is chosen
/// in it. Whether the text was changed (a suggestion taken in place of a word).
pub(crate) fn while_typing(
    ed: &mut crate::editor::Editor,
    ctx: &egui::Context,
    id: ObjectId,
    shown: &egui::text_edit::TextEditOutput,
    text: &mut String,
    escape: bool,
) -> bool {
    let on = !ed.settings.no_spell_check;
    if on {
        prepare();
    }
    let found = if on { ed.spell.of(text, &ed.settings.own_words) } else { Vec::new() };
    let (galley, origin) = (&shown.galley, shown.galley_pos);
    ed.spell.shown = found.iter().flat_map(|m| rows(galley, origin, &m.chars).into_iter().map(move |r| (m.clone(), r))).collect();
    let painter = ctx.layer_painter(shown.response.layer_id).with_clip_rect(ed.view.canvas);
    underline(&painter, &ed.spell.shown.iter().map(|(_, r)| *r).collect::<Vec<_>>());
    // A right click on the box opens the menu, about the word under the pointer if it is unknown.
    if shown.response.secondary_clicked()
        && let Some(p) = shown.response.interact_pointer_pos()
    {
        let at = galley.cursor_from_pos(p - origin).index.0;
        let word = found.iter().find(|m| m.chars.contains(&at) || m.chars.end == at).cloned().map(|m| {
            let offered = suggestions(&m.word);
            (m, offered)
        });
        ed.spell.menu = Some(Menu { id, at: p, word });
        return false;
    }
    let Some(menu) = ed.spell.menu.clone().filter(|m| m.id == id) else {
        ed.spell.menu = None;
        return false;
    };
    let (mut take, mut add, mut turn) = (None, false, None);
    let popup = egui::Popup::new(egui::Id::new("spelling-menu"), ctx.clone(), egui::PopupAnchor::Position(menu.at), shown.response.layer_id)
        .kind(egui::PopupKind::Menu)
        .layout(egui::Layout::top_down_justified(egui::Align::Min))
        .style(egui::containers::menu::menu_style)
        .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
        .open(true)
        .show(|ui| {
            ui.set_min_width(170.0);
            if let Some((m, offered)) = &menu.word {
                if offered.is_empty() {
                    ui.add_enabled(false, egui::Button::new(NO_SUGGESTIONS));
                }
                for s in offered {
                    if ui
                        .add(egui::Button::new(egui::RichText::new(s.as_str()).strong()))
                        .on_hover_text(format!("In place of “{}”", m.word))
                        .clicked()
                    {
                        take = Some(s.clone());
                    }
                }
                ui.separator();
                if ui.button(ADD_WORD).on_hover_text(format!("“{}” isn't marked again", m.word)).clicked() {
                    add = true;
                }
                ui.separator();
            }
            let mut checking = on;
            if ui.checkbox(&mut checking, CHECK_SPELLING).changed() {
                turn = Some(checking);
            }
        });
    // A press anywhere else, or Esc, closes it.
    let rect = popup.map(|p| p.response.rect);
    let elsewhere = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !rect.is_some_and(|r| r.contains(p))));
    let mut close = escape || elsewhere;
    let mut changed = false;
    if let (Some(with), Some((m, _))) = (take, &menu.word) {
        let (t, cursor) = replaced(text, &m.chars, &with);
        *text = t;
        set_cursor(ctx, shown.response.id, cursor);
        changed = true;
        close = true;
    }
    if add && let Some((m, _)) = &menu.word {
        if ed.settings.add_word(&m.word) {
            ed.services.save_settings(&ed.settings);
        }
        close = true;
    }
    if let Some(checking) = turn {
        ed.settings.no_spell_check = !checking;
        ed.services.save_settings(&ed.settings);
        close = true;
    }
    if close {
        ed.spell.menu = None;
    }
    changed
}

/// The red wavy line under each part of `rows` (a word's rectangle on each row it is on).
pub fn underline(p: &egui::Painter, rows: &[Rect]) {
    let red = Color32::from_rgb(0xE5, 0x39, 0x35);
    for r in rows {
        let (x0, x1, y) = (r.left(), r.right(), r.bottom() - 1.5);
        if !(x0.is_finite() && x1.is_finite() && y.is_finite()) || x1 - x0 < 1.0 {
            continue;
        }
        let mut points = Vec::new();
        let mut x = x0;
        let mut up = true;
        while x <= x1 && points.len() < 4_000 {
            points.push(pos2(x, if up { y - 1.25 } else { y + 1.25 }));
            up = !up;
            x += 2.0;
        }
        p.add(egui::Shape::line(points, Stroke::new(1.1, red)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<String> {
        words_in(text).into_iter().map(|(_, w)| w).collect()
    }

    #[test]
    fn words_are_the_letters_between_spaces_without_their_punctuation() {
        assert_eq!(found("Hello, (world)! It's “fine”."), vec!["Hello", "world", "It's", "fine"]);
        assert_eq!(found("well-known  e-mail"), vec!["well", "known", "mail"]);
        // Numbers, addresses, files and abbreviations aren't words to check; nor is one letter.
        assert!(found("3rd 2024 a@b.com www.example.org notes.txt NASA OK I a C3PO").is_empty());
        let w = words_in("¿Qué tal?  héllo");
        assert_eq!(w, vec![(1..4, "Qué".to_string()), (5..8, "tal".to_string()), (11..16, "héllo".to_string())]);
        assert!(words_in("").is_empty() && words_in("   \n\t").is_empty() && words_in("--- ''' …").is_empty());
    }

    #[test]
    fn a_word_replaced_keeps_the_rest_and_says_where_the_cursor_goes() {
        let (t, at) = replaced("Teh cat", &(0..3), "The");
        assert_eq!((t.as_str(), at), ("The cat", 3));
        let (t, at) = replaced("a mispeled word", &(2..10), "misspelled");
        assert_eq!((t.as_str(), at), ("a misspelled word", 12));
        let (t, at) = replaced("short", &(3..99), "rt");
        assert_eq!((t.as_str(), at), ("short", 5));
    }

    #[test]
    fn the_word_list_knows_english_and_offers_corrections() {
        assert!(ready_now(), "the word list reads");
        let wrong: Vec<String> = misspelt("Teh quick brwon fox jumps over the lazy dog's back.", &[]).into_iter().map(|m| m.word).collect();
        assert_eq!(wrong, vec!["Teh", "brwon"]);
        assert!(suggestions("brwon").iter().any(|s| s == "brown"), "{:?}", suggestions("brwon"));
        assert!(suggestions("Teh").iter().any(|s| s == "The"), "{:?}", suggestions("Teh"));
        // The owner's own words are known, whatever their case.
        assert!(misspelt("ScreenGrabCraft rocks", &["screengrabcraft".to_string()]).is_empty());
        assert_eq!(misspelt("ScreenGrabCraft rocks", &[]).len(), 1);
        // Long nonsense is let be; checking it and asking for suggestions don't panic.
        let long = "x".repeat(10_000);
        assert!(misspelt(&long, &[]).is_empty());
        assert!(suggestions(&long).is_empty());
        for junk in ["\u{0}\u{1}", "ééééééééé", "q'q'q'q", "ʼʼʼʼ", "ǅungla", "İstanbul", "ß", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"] {
            let _ = misspelt(junk, &[]);
            let _ = suggestions(junk);
        }
    }

    #[test]
    fn the_unknown_words_are_checked_again_only_when_the_text_changes() {
        assert!(ready_now());
        let mut s = State::default();
        assert_eq!(s.of("a mispeled word", &[]).len(), 1);
        assert_eq!(s.of("a mispeled word", &[]).len(), 1);
        assert!(s.of("a misspelled word", &[]).is_empty());
        assert!(s.of("a mispeled word", &["mispeled".to_string()]).is_empty(), "the owner's words changed");
    }
}
