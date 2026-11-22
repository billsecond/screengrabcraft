//! Zoom and pan: an item of a layer that zooms the screen's picture into part of it for as long
//! as it lasts. It eases in from the whole picture and back out again (its entrance and exit say
//! how long; None is a cut), and when one zoom touches the next the picture pans straight from
//! one to the other without going out in between. Only the screen (and the pointer on it) is
//! zoomed: the camera, the layers' items and the captions stay where they are on the video.

use crate::layers::{AnimKind, Item, LayerKind, What};
use crate::timeline::Timeline;

/// The part of the screen shown, in shares of the picture: its left, top, width and height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl View {
    /// All of the picture.
    pub const WHOLE: View = View { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };

    /// Whether it shows (nearly) all of the picture.
    pub fn is_whole(&self) -> bool {
        self.w >= 0.999 && self.h >= 0.999
    }

    fn lerp(a: View, b: View, f: f32) -> View {
        let f = if f.is_finite() { f.clamp(0.0, 1.0) } else { 1.0 };
        let m = |p: f32, q: f32| p + (q - p) * f;
        View { x: m(a.x, b.x), y: m(a.y, b.y), w: m(a.w, b.w), h: m(a.h, b.h) }
    }

    /// The same view on a picture `w × h`: its left, top, width and height in pixels, kept on it.
    pub fn pixels(&self, w: u32, h: u32) -> (f32, f32, f32, f32) {
        let (fw, fh) = (w.max(1) as f32, h.max(1) as f32);
        let vw = (self.w * fw).clamp(1.0, fw);
        let vh = (self.h * fh).clamp(1.0, fh);
        ((self.x * fw).clamp(0.0, fw - vw), (self.y * fh).clamp(0.0, fh - vh), vw, vh)
    }
}

/// The least of the picture a zoom shows: ten times larger.
pub const MIN_SHARE: f32 = 0.1;

/// How long a zoom takes to go in, and out, unless set otherwise.
pub const EASE_US: u64 = 600_000;

/// The share of the picture a zoom of this size shows (its width and height alike, as it keeps
/// the video's shape).
pub fn share(w: f32) -> f32 {
    if w.is_finite() { w.clamp(MIN_SHARE, 1.0) } else { 1.0 }
}

/// What a zoom item shows once it's in: its frame, the video's shape, about its middle and kept
/// on the picture.
pub fn frame(item: &Item) -> View {
    let s = share(item.place.w);
    let mid = |v: f32| if v.is_finite() { v } else { 0.5 };
    let x = (mid(item.place.x) - s / 2.0).clamp(0.0, 1.0 - s);
    let y = (mid(item.place.y) - s / 2.0).clamp(0.0, 1.0 - s);
    View { x, y, w: s, h: s }
}

/// Slow at both ends, as a camera moves.
fn ease(f: f32) -> f32 {
    let f = if f.is_finite() { f.clamp(0.0, 1.0) } else { 1.0 };
    f * f * (3.0 - 2.0 * f)
}

/// Every zoom of the shown picture layers, by when it starts.
fn zooms(timeline: &Timeline) -> Vec<&Item> {
    let mut v: Vec<&Item> = timeline
        .layers
        .iter()
        .filter(|l| l.kind == LayerKind::Picture && !l.hidden)
        .flat_map(|l| l.items.iter())
        .filter(|i| matches!(i.what, What::Zoom) && i.len > 0)
        .collect();
    v.sort_by_key(|i| (i.at, i.end()));
    v
}

/// How long a zoom's way in (or out) lasts: none for a cut, at most half the zoom.
fn ease_len(item: &Item, kind: AnimKind, len: u64) -> u64 {
    if kind == AnimKind::None { 0 } else { len.min(item.len / 2) }
}

/// The part of the screen shown at timeline time `t`.
pub fn view_at(timeline: &Timeline, t: u64) -> View {
    let zs = zooms(timeline);
    // The zoom that began last of those under the playhead.
    let Some(i) = zs.iter().rposition(|z| z.at <= t && t < z.end()) else { return View::WHOLE };
    let Some(z) = zs.get(i) else { return View::WHOLE };
    // Where its keyframes have it then (a zoom with them pans along them).
    let target = frame(&z.placed_at(t));
    // Touching (or under) the one before, it pans from there; touching the next, it stays in
    // until that one takes over.
    let before = i.checked_sub(1).and_then(|p| zs.get(p)).filter(|p| p.end() >= z.at).map(|p| frame(&p.placed_at(p.end())));
    let after = zs.get(i + 1).is_some_and(|n| n.at <= z.end());
    let (into, left) = (t.saturating_sub(z.at), z.end().saturating_sub(t));
    let len_in = ease_len(z, z.intro.kind, z.intro.len);
    let len_out = ease_len(z, z.outro.kind, z.outro.len);
    if into < len_in {
        return View::lerp(before.unwrap_or(View::WHOLE), target, ease(into as f32 / len_in as f32));
    }
    if !after && left < len_out {
        return View::lerp(View::WHOLE, target, ease(left as f32 / len_out as f32));
    }
    target
}

/// Whether any zoom is on the timeline.
pub fn any(timeline: &Timeline) -> bool {
    !zooms(timeline).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::{Anim, Layer, Placement};

    fn zoom(at: u64, len: u64, x: f32, y: f32, w: f32, ease: u64) -> Item {
        let kind = if ease == 0 { AnimKind::None } else { AnimKind::Fade };
        let a = Anim { kind, len: ease };
        Item { at, len, what: What::Zoom, place: Placement { x, y, w, opacity: 1.0 }, intro: a, outro: a, ..Item::default() }
    }

    fn timeline(items: Vec<Item>) -> Timeline {
        let mut tl = Timeline {
            tracks: Vec::new(),
            camera: Default::default(),
            captions: Default::default(),
            layers: Vec::new(),
            media: Vec::new(),
            pointer: Default::default(),
            corrected: None,
            markers: Vec::new(),
            keys: Default::default(),
            ducking: Default::default(),
            watermark: Default::default(),
            frame: Default::default(),
            motion_blur: Default::default(),
            pointer_hidden: Vec::new(),
        };
        tl.layers.push(Layer { id: 1, name: "Zoom".to_string(), items, ..Layer::default() });
        tl
    }

    #[test]
    fn a_zoom_eases_in_stays_and_eases_out() {
        let tl = timeline(vec![zoom(1_000_000, 3_000_000, 0.5, 0.5, 0.5, 500_000)]);
        assert_eq!(view_at(&tl, 0), View::WHOLE);
        assert_eq!(view_at(&tl, 1_000_000), View::WHOLE, "it starts from the whole picture");
        let half = view_at(&tl, 1_250_000);
        assert!(half.w < 1.0 && half.w > 0.5, "{half:?}");
        assert_eq!(view_at(&tl, 2_000_000), View { x: 0.25, y: 0.25, w: 0.5, h: 0.5 });
        let leaving = view_at(&tl, 3_750_000);
        assert!(leaving.w < 1.0 && leaving.w > 0.5, "{leaving:?}");
        assert_eq!(view_at(&tl, 4_000_000), View::WHOLE);
    }

    fn near(a: View, b: View) -> bool {
        (a.x - b.x).abs() < 1e-5 && (a.y - b.y).abs() < 1e-5 && (a.w - b.w).abs() < 1e-5 && (a.h - b.h).abs() < 1e-5
    }

    #[test]
    fn touching_zooms_pan_from_one_to_the_next_without_going_out() {
        let a = zoom(0, 2_000_000, 0.25, 0.25, 0.4, 400_000);
        let b = zoom(2_000_000, 2_000_000, 0.75, 0.75, 0.4, 400_000);
        let tl = timeline(vec![a, b]);
        // The end of the first stays in (no way out), the second starts from it.
        assert!(near(view_at(&tl, 1_900_000), View { x: 0.05, y: 0.05, w: 0.4, h: 0.4 }), "{:?}", view_at(&tl, 1_900_000));
        assert!(near(view_at(&tl, 2_000_000), View { x: 0.05, y: 0.05, w: 0.4, h: 0.4 }), "{:?}", view_at(&tl, 2_000_000));
        let mid = view_at(&tl, 2_200_000);
        assert!(mid.x > 0.05 && mid.x < 0.55 && (mid.w - 0.4).abs() < 1e-6, "{mid:?}");
        assert!(near(view_at(&tl, 3_000_000), View { x: 0.55, y: 0.55, w: 0.4, h: 0.4 }), "{:?}", view_at(&tl, 3_000_000));
    }

    #[test]
    fn a_cut_goes_straight_in_and_frames_stay_on_the_picture() {
        let tl = timeline(vec![zoom(0, 1_000_000, 0.98, 0.02, 0.3, 0)]);
        // Pushed back onto the picture at its corner.
        assert_eq!(view_at(&tl, 0), View { x: 0.7, y: 0.0, w: 0.3, h: 0.3 });
        // Junk: never too close, never off the picture, never NaN.
        let junk = Item { place: Placement { x: f32::NAN, y: f32::INFINITY, w: -4.0, opacity: 1.0 }, ..zoom(0, 1, 0.0, 0.0, 0.0, 0) };
        let v = frame(&junk);
        assert!(v.w >= MIN_SHARE && v.x >= 0.0 && v.y >= 0.0 && v.x + v.w <= 1.0 && v.y + v.h <= 1.0, "{v:?}");
        assert_eq!(View { x: 0.5, y: 0.5, w: 0.5, h: 0.5 }.pixels(100, 50), (50.0, 25.0, 50.0, 25.0));
    }

    #[test]
    fn hidden_layers_dont_zoom() {
        let mut tl = timeline(vec![zoom(0, 1_000_000, 0.5, 0.5, 0.5, 0)]);
        assert!(any(&tl));
        if let Some(l) = tl.layers.first_mut() {
            l.hidden = true;
        }
        assert_eq!(view_at(&tl, 500_000), View::WHOLE);
        assert!(!any(&tl));
    }
}
