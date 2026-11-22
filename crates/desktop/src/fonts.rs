//! The fonts on the computer, for text and callouts: found in the system's font folders by
//! fontdb (MIT), read when a text needs one, and never copied anywhere. A project keeps a font's
//! name; on a computer without it, its text is drawn in the bundled font.

use std::sync::OnceLock;

use screengrabcraft_render::text::FontSource;

/// The fonts found, looked for once (the first to ask waits for the look).
fn database() -> &'static fontdb::Database {
    static DB: OnceLock<fontdb::Database> = OnceLock::new();
    DB.get_or_init(|| {
        let started = std::time::Instant::now();
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        log::info!("{} fonts on this computer, found in {} ms", db.len(), started.elapsed().as_millis());
        db
    })
}

/// The most families listed.
const MAX_FAMILIES: usize = 4000;

/// The fonts on the computer, for [`screengrabcraft_render::text`].
struct SystemFonts;

impl FontSource for SystemFonts {
    fn face(&self, family: &str, bold: bool) -> Option<(Vec<u8>, u32)> {
        let db = database();
        let families = [fontdb::Family::Name(family)];
        let query = fontdb::Query {
            families: &families,
            weight: if bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL },
            stretch: fontdb::Stretch::Normal,
            style: fontdb::Style::Normal,
        };
        let id = db.query(&query)?;
        db.with_face_data(id, |data, index| (data.to_vec(), index))
    }

    fn families(&self) -> Vec<String> {
        let mut names: Vec<String> =
            database().faces().filter_map(|f| f.families.first().map(|(name, _)| name.clone())).filter(|n| !n.trim().is_empty()).collect();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        names.truncate(MAX_FAMILIES);
        names
    }
}

/// Lets text and callouts use the fonts on the computer, and starts looking for them now, out of
/// the way, so they are there when a font is first picked.
pub fn install() {
    if screengrabcraft_render::text::set_font_source(Box::new(SystemFonts)) {
        let _ = std::thread::Builder::new().name("fonts".into()).spawn(|| {
            database();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fonts on this computer are found, and one of them reads as text draws it and as the
    /// editor shows it being typed (`cargo test -p screengrabcraft-desktop live_fonts -- --ignored`;
    /// it reads the system's font files, nothing of the owner's).
    #[test]
    #[ignore = "reads the fonts installed on this computer"]
    fn live_fonts_are_found_and_read() {
        let families = SystemFonts.families();
        assert!(!families.is_empty(), "this computer has fonts");
        let first = families.first().cloned().unwrap();
        let (bytes, index) = SystemFonts.face(&first, false).expect("its file");
        assert!(ab_glyph::FontVec::try_from_vec_and_index(bytes, index).is_ok(), "{first} reads");
        assert!(SystemFonts.face("No Such Font Anywhere", false).is_none());
        println!("{} families, the first {first}", families.len());
    }
}
