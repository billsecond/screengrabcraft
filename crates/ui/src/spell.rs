//! Spelling, checked while a text or a callout is typed: the words the English word list doesn't
//! know get a red wavy line under them, and a right click on one offers the list's suggestions,
//! Add to Dictionary (the word kept in the editor's settings) and the setting that turns checking
//! off (Edit › Check Spelling has it too).
//!
//! The checker is `symspell` (MIT); the word list is SymSpell's English frequency dictionary (MIT,
//! made from Google Books Ngram data and SCOWL; `assets/dictionaries`, its notices in `NOTICE.txt`,
//! shown in About), built into the program. It knows US and UK spellings and the usual
//! contractions. The checker finds the words one letter away from one typed (a letter wrong,
//! missing or one too many; two would take some 150 MB to hold); two letters the wrong way round
//! and a missing apostrophe are tried here, and the suggestions are put in order of how much each
//! word is used. Reading it takes a moment, so it is read once, on a thread of its own where there
//! are threads (in the browser, at once): words are underlined once it is ready.
//!
//! Typed words are hostile like anything else from outside: only plain letters a to z and
//! apostrophes, lower case and no longer than [`MAX_WORD`], go into the checker; a word with other
//! letters (an accent) is let be, as are the longest, and a checker that panics counts as knowing
//! the word.

use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::text::CCursor;
use egui::{Color32, Pos2, Rect, Stroke, pos2};
use screengrabcraft_model::ObjectId;
use symspell::{SymSpell, SymSpellBuilder, UnicodeStringStrategy, Verbosity};

/// The word list: a word and how often it is used, a line each.
const LIST: &str = include_str!("../../../assets/dictionaries/frequency_dictionary_en_82_765.txt");
/// Where the word list and the checker come from, and their licences (About shows them).
pub const NOTICE: &str = include_str!("../../../assets/dictionaries/NOTICE.txt");

type Checker = SymSpell<UnicodeStringStrategy>;

static WORDS: OnceLock<Option<Checker>> = OnceLock::new();
static STARTED: AtomicBool = AtomicBool::new(false);

/// The longest word checked, and the longest given suggestions, in characters.
pub const MAX_WORD: usize = 48;
const MAX_SUGGESTED: usize = 32;
/// The most words of one text checked.
const MAX_WORDS: usize = 5_000;
/// The most suggestions offered.
pub const MAX_SUGGESTIONS: usize = 6;
/// How far a suggestion the checker finds may be from the word typed, in letters.
const DISTANCE: i64 = 1;
/// The most lines of the word list read (it has some 83,000).
const MAX_LIST: usize = 200_000;

/// What the menu's items are called.
pub const ADD_WORD: &str = "Add to Dictionary";
pub const CHECK_SPELLING: &str = "Check Spelling";
pub const NO_SUGGESTIONS: &str = "No suggestions";

/// Whether `word` may go into the checker: letters a to z, lower case, with apostrophes, at most
/// [`MAX_WORD`] long.
fn checkable(word: &str) -> bool {
    word.len() <= MAX_WORD && word.bytes().any(|b| b.is_ascii_lowercase()) && word.bytes().all(|b| b.is_ascii_lowercase() || b == b'\'')
}

fn read() -> Option<Checker> {
    catch_unwind(|| {
        let mut list: Checker = SymSpellBuilder::<UnicodeStringStrategy>::default().max_dictionary_edit_distance(DISTANCE).build().ok()?;
        for line in LIST.trim_start_matches('\u{feff}').lines().take(MAX_LIST) {
            let line = line.trim_end_matches('\r');
            let mut parts = line.split(' ');
            if let (Some(word), Some(count), None) = (parts.next(), parts.next(), parts.next())
                && checkable(word)
                && count.parse::<i64>().is_ok_and(|n| n > 0)
            {
                list.load_dictionary_line(line, 0, 1, " ");
            }
        }
        Some(list)
    })
    .ok()
    .flatten()
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
fn words() -> Option<&'static Checker> {
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

/// `word` as the checker takes it: lower case, a curly apostrophe made straight; none if it has
/// letters other than a to z (an accent), or is too long, so it is let be.
fn plain(word: &str) -> Option<String> {
    let w = word.chars().map(|c| if is_apostrophe(c) { '\'' } else { c }).collect::<String>().to_lowercase();
    checkable(&w).then_some(w)
}

/// The word list's own entry for `word` (made plain), if it has it; an error if the checker
/// failed on it.
fn entry(list: &Checker, word: &str) -> Result<Option<symspell::Suggestion>, ()> {
    catch_unwind(AssertUnwindSafe(|| list.lookup(word, Verbosity::Top, 0))).map(|found| found.into_iter().find(|s| s.distance == 0)).map_err(|_| ())
}

/// Whether `word` is in the word list (as it is, or as the dog's or the dogs' are) or the owner's
/// words.
fn known(list: &Checker, word: &str, own: &[String]) -> bool {
    if word.chars().count() > MAX_WORD || own.iter().any(|o| o.to_lowercase() == word.to_lowercase()) {
        return true;
    }
    let Some(w) = plain(word) else { return true };
    // Known, or the checker failed on it.
    if !matches!(entry(list, &w), Ok(None)) {
        return true;
    }
    let stem = w.strip_suffix("'s").or_else(|| w.strip_suffix('\''));
    stem.is_some_and(|s| checkable(s) && !matches!(entry(list, s), Ok(None)))
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
    let Some(list) = words() else { return Vec::new() };
    words_in(text).into_iter().filter(|(_, w)| !known(list, w, own)).map(|(chars, word)| Misspelt { chars, word }).collect()
}

/// What the word list offers in place of `word`, best first: a missing apostrophe put in, then
/// the words a letter away or with two letters swapped, the most used first; with a capital first
/// letter if `word` has one (and "I" always).
pub fn suggestions(word: &str) -> Vec<String> {
    let Some(list) = words() else { return Vec::new() };
    if word.chars().count() > MAX_SUGGESTED {
        return Vec::new();
    }
    let Some(w) = plain(word) else { return Vec::new() };
    let chars: Vec<char> = w.chars().collect();
    // Each: the word, how much it is used, whether it is `w` with an apostrophe put in.
    let near = catch_unwind(AssertUnwindSafe(|| list.lookup(&w, Verbosity::All, DISTANCE))).unwrap_or_default();
    let mut all: Vec<(String, i64, bool)> = near.into_iter().filter(|s| s.distance > 0).map(|s| (s.term, s.count, false)).collect();
    // Two letters the wrong way round (teh: the).
    for i in 0..chars.len().saturating_sub(1) {
        let mut swapped = chars.clone();
        swapped.swap(i, i + 1);
        let q: String = swapped.into_iter().collect();
        if q != w
            && let Ok(Some(s)) = entry(list, &q)
        {
            all.push((s.term, s.count, false));
        }
    }
    // An apostrophe left out (dont: don't).
    for i in 1..chars.len() {
        let q: String = chars.iter().take(i).chain(std::iter::once(&'\'')).chain(chars.iter().skip(i)).collect();
        if let Ok(Some(s)) = entry(list, &q) {
            all.push((s.term, s.count, true));
        }
    }
    all.sort_by_key(|(_, count, apostrophe)| (!*apostrophe, std::cmp::Reverse(*count)));
    let capital = word.chars().next().is_some_and(char::is_uppercase);
    let mut out: Vec<String> = Vec::new();
    for (s, _, _) in all {
        let s = if capital || s == "i" || s.starts_with("i'") { capitalised(&s) } else { s };
        if !s.is_empty() && s.chars().count() <= MAX_WORD && !out.contains(&s) {
            out.push(s);
        }
        if out.len() >= MAX_SUGGESTIONS {
            break;
        }
    }
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

    #[test]
    fn us_and_uk_spellings_contractions_and_possessives_are_known() {
        assert!(ready_now());
        let text = "The colour and color, realise and realize; don’t, it's, I'm, the dogs' bowls and the cat's.";
        assert!(misspelt(text, &[]).is_empty(), "{:?}", misspelt(text, &[]));
    }

    #[test]
    fn a_missing_apostrophe_and_swapped_letters_are_offered_first() {
        assert!(ready_now());
        assert_eq!(suggestions("dont").first().map(String::as_str), Some("don't"), "{:?}", suggestions("dont"));
        assert_eq!(suggestions("teh").first().map(String::as_str), Some("the"), "{:?}", suggestions("teh"));
        assert_eq!(suggestions("Teh").first().map(String::as_str), Some("The"), "{:?}", suggestions("Teh"));
        assert_eq!(suggestions("im").first().map(String::as_str), Some("I'm"), "{:?}", suggestions("im"));
        assert!(suggestions("recieve").iter().any(|s| s == "receive"), "{:?}", suggestions("recieve"));
        assert!(suggestions("teh").len() <= MAX_SUGGESTIONS);
        // A word with an accent is let be: never underlined, nothing offered.
        assert!(misspelt("naïve café", &[]).is_empty());
        assert!(suggestions("café").is_empty());
    }

    #[test]
    fn only_plain_words_reach_the_checker_and_junk_never_panics() {
        assert!(ready_now());
        for (word, ok) in
            [("don't", true), ("abc", true), ("", false), ("'''", false), ("café", false), ("ABC", false), ("a-b", false), ("a b", false)]
        {
            assert_eq!(checkable(word), ok, "{word}");
        }
        assert!(!checkable(&"a".repeat(MAX_WORD + 1)) && checkable(&"a".repeat(MAX_WORD)));
        assert_eq!(plain("Don’t").as_deref(), Some("don't"));
        // Thousands of made-up words of letters, apostrophes and odd characters, checked and
        // offered corrections: nothing panics, nothing offered is longer than a word may be.
        let parts = ['a', 'e', 'z', 'q', '\'', '’', 'é', 'ß', 'I', '\u{0}', 'x', 'o'];
        let mut seed: u32 = 7;
        for _ in 0..3_000 {
            let mut w = String::new();
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            for k in 0..(1 + seed % 12) {
                let i = (seed >> (k % 16)) as usize % parts.len();
                w.push(parts.get(i).copied().unwrap_or('a'));
            }
            let _ = misspelt(&w, &[]);
            assert!(suggestions(&w).iter().all(|s| s.chars().count() <= MAX_WORD));
        }
    }
}
