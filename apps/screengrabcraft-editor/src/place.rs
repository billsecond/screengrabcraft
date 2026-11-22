//! Where the window was left, as eframe keeps it (in `editor-window.ron`), and only ever a place
//! the owner left it in: shown, not minimized, at least as big as the editor is ever shown.
//!
//! eframe writes the window's place whatever state the window is in, now and then and as the
//! program ends. A window that's minimized, or kept out of sight, sits at Windows' "nowhere"
//! (-32000, -32000), squeezed. The next start "rescues" that by moving it onto the first screen
//! and making it at least 64 points: a tiny window stuck at the top left of the first screen. So
//! the editor writes over eframe's with the last good place it saw (`App::save` comes after
//! eframe's own, before the file is written).

use serde::{Deserialize, Serialize};

/// eframe's key for the window's place in its storage.
pub const KEY: &str = "window";

/// The longest text read back as a place: eframe's is about 200 characters.
const MAX_TEXT: usize = 4096;

/// Windows keeps a minimized window at -32000; nothing on a screen is that far off.
const NOWHERE: f32 = 30_000.0;

/// A point or a size, in eframe's form (`egui::Pos2` and `egui::Vec2` are kept as `(x:…,y:…)`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Xy {
    pub x: f32,
    pub y: f32,
}

/// The window's place, with the same fields and names as eframe's (`egui_winit::WindowSettings`),
/// so that eframe reads back what's written here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Place {
    /// Where the window's inside is, in pixels.
    pub inner_position_pixels: Option<Xy>,
    /// Where its frame is, in pixels.
    pub outer_position_pixels: Option<Xy>,
    pub fullscreen: bool,
    pub maximized: bool,
    /// The size of its inside, in points.
    pub inner_size_points: Option<Xy>,
}

/// A position somewhere on a screen (not Windows' "nowhere" of a minimized window).
fn on_a_screen(p: Option<Xy>) -> bool {
    p.is_none_or(|p| p.x.is_finite() && p.y.is_finite() && p.x.abs() < NOWHERE && p.y.abs() < NOWHERE)
}

impl Place {
    /// A place the owner left the window in: on a screen, and at least `min` points (the smallest
    /// the editor is ever shown at), or maximized or full screen.
    pub fn is_good(&self, min: (f32, f32)) -> bool {
        let sized = self.inner_size_points.is_some_and(|s| {
            s.x.is_finite()
                && s.y.is_finite()
                && s.x < NOWHERE
                && s.y < NOWHERE
                && ((s.x >= min.0 - 1.0 && s.y >= min.1 - 1.0) || ((self.maximized || self.fullscreen) && s.x >= 1.0 && s.y >= 1.0))
        });
        sized && on_a_screen(self.inner_position_pixels) && on_a_screen(self.outer_position_pixels)
    }

    /// A place as eframe wrote it; None for anything else.
    pub fn read(text: &str) -> Option<Place> {
        if text.len() > MAX_TEXT {
            return None;
        }
        ron::from_str(text).ok()
    }

    /// The place as eframe writes it.
    pub fn write(&self) -> Option<String> {
        ron::to_string(self).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: (f32, f32) = (760.0, 480.0);

    fn xy(x: f32, y: f32) -> Option<Xy> {
        Some(Xy { x, y })
    }

    fn place(at: (f32, f32), size: (f32, f32)) -> Place {
        Place {
            inner_position_pixels: xy(at.0 + 8.0, at.1 + 31.0),
            outer_position_pixels: xy(at.0, at.1),
            fullscreen: false,
            maximized: false,
            inner_size_points: xy(size.0, size.1),
        }
    }

    #[test]
    fn where_the_owner_left_it_is_kept() {
        // As eframe wrote the owner's window: big, on the first screen.
        let text = "(inner_position_pixels:Some((x:464.0,y:328.0)),outer_position_pixels:Some((x:456.0,y:297.0)),fullscreen:false,maximized:false,inner_size_points:Some((x:2838.0,y:1678.0)))";
        let p = Place::read(text);
        assert_eq!(
            p,
            Some(place((456.0, 297.0), (2838.0, 1678.0))).map(|mut p| {
                p.inner_position_pixels = xy(464.0, 328.0);
                p
            })
        );
        assert!(p.is_some_and(|p| p.is_good(MIN)));
        // On a screen left of or above the first one (negative places), and the smallest size.
        assert!(place((-3840.0, -400.0), (1320.0, 860.0)).is_good(MIN));
        assert!(place((10.0, 10.0), (760.0, 480.0)).is_good(MIN));
        // Maximized or full screen, whatever size it says.
        let mut big = place((-8.0, -8.0), (2560.0, 1380.0));
        big.maximized = true;
        assert!(big.is_good(MIN));
    }

    #[test]
    fn a_minimized_or_squeezed_window_is_never_kept() {
        // Minimized: Windows' "nowhere", squeezed to a title bar.
        assert!(!place((-32000.0, -32000.0), (160.0, 28.0)).is_good(MIN));
        assert!(!place((-32000.0, -32000.0), (1320.0, 860.0)).is_good(MIN), "nowhere, at any size");
        assert!(!place((-32000.0, -32000.0), (0.0, 0.0)).is_good(MIN));
        // What eframe made of that: a 64-point window at the top left of the first screen.
        assert!(!place((0.0, 0.0), (64.0, 64.0)).is_good(MIN));
        assert!(!place((0.0, 0.0), (70.0, 60.0)).is_good(MIN));
        // Too narrow or too short, even on a screen.
        assert!(!place((100.0, 100.0), (2000.0, 300.0)).is_good(MIN));
        // Maximized, but nowhere.
        let mut gone = place((-32000.0, -32000.0), (160.0, 28.0));
        gone.maximized = true;
        assert!(!gone.is_good(MIN));
        // No size at all.
        let mut sizeless = place((10.0, 10.0), (1320.0, 860.0));
        sizeless.inner_size_points = None;
        assert!(!sizeless.is_good(MIN));
    }

    #[test]
    fn junk_is_never_a_place() {
        for text in ["", "junk", "(", "(inner_size_points:Some((x:\"a\",y:1.0)))", "[1,2,3]", "Some(())"] {
            assert!(Place::read(text).is_none_or(|p| !p.is_good(MIN)), "{text:?}");
        }
        assert_eq!(Place::read(&"(".repeat(MAX_TEXT + 1)), None, "too long to be one");
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0e9, -1.0e9] {
            assert!(!place((bad, 10.0), (1320.0, 860.0)).is_good(MIN));
            assert!(!place((10.0, 10.0), (bad, 860.0)).is_good(MIN));
        }
    }

    #[test]
    fn eframe_reads_back_what_is_written() {
        let p = place((-1920.0, 120.0), (1500.25, 900.5));
        let text = p.write();
        assert!(text.is_some());
        let text = text.unwrap_or_default();
        // eframe's own reading of it: the same size.
        let theirs: Result<egui_winit::WindowSettings, _> = ron::from_str(&text);
        assert!(theirs.is_ok(), "{text}");
        assert_eq!(theirs.ok().and_then(|s| s.inner_size_points()), Some(egui::vec2(1500.25, 900.5)));
        // And this one reads it back unchanged.
        assert_eq!(Place::read(&text), Some(p));
        // With no good place seen, "()" is kept: no place and no size, so eframe opens the window
        // at its usual size where Windows puts new windows.
        let none: Result<egui_winit::WindowSettings, _> = ron::from_str("()");
        assert!(none.is_ok());
        assert_eq!(none.ok().and_then(|s| s.inner_size_points()), None);
        assert!(Place::read("()").is_some_and(|p| !p.is_good(MIN)));
    }
}
