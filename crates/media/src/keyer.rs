//! The camera's background taken away, so the screen shows round the person:
//!
//! - **A colour removed** (a green screen, [`colour_key`]): each pixel of the camera's picture is
//!   kept by how far its colour (its hue and strength, not its brightness, so the screen's shadows
//!   go too) is from the colour chosen, softly at the edge, and that colour's glow on the person
//!   is taken out.
//! - **The background removed** (no green screen, [`plate_key`]): the background model runs on a
//!   few of the camera's pictures ([`learn_from`]: seconds each, far too slow for every picture).
//!   Where it says background, what the camera saw there is kept as a picture of the background
//!   (a [`Plate`]). Each picture of the video then keeps what differs from that background, which
//!   follows the person as they move and costs next to nothing; what the model never saw
//!   uncovered (the person was always there) is kept as the person. The camera has to stay still:
//!   one that moves isn't followed.

use screengrabcraft_model::Raster;
use screengrabcraft_render::matte::{Matte, Matting};
use serde::{Deserialize, Serialize};

/// What's taken away from the camera's picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Remove {
    #[default]
    Nothing,
    /// One colour (a green screen).
    Colour,
    /// The background, learnt by the background model (no green screen).
    Background,
}

/// A green screen's green.
pub const GREEN: [u8; 3] = [0x00, 0xB1, 0x40];

/// A colour's hue and strength (Cb, Cr): brightness left out.
fn chroma(r: f32, g: f32, b: f32) -> (f32, f32) {
    (-0.1687 * r - 0.3313 * g + 0.5 * b, 0.5 * r - 0.4187 * g - 0.0813 * b)
}

/// How much of each pixel of `pic` is kept (255 all of it) with the colour `key` taken away;
/// `tolerance` (0 to 1) is how far from it a colour still goes. The key's glow on what's kept is
/// taken out of `pic` itself.
pub fn colour_key(pic: &mut Raster, key: [u8; 3], tolerance: f32) -> Vec<u8> {
    let tolerance = if tolerance.is_finite() { tolerance.clamp(0.0, 1.0) } else { 0.35 };
    let [kr, kg, kb] = key.map(f32::from);
    let (kcb, kcr) = chroma(kr, kg, kb);
    let inner = 8.0 + tolerance * 90.0;
    let outer = inner + 22.0;
    // The key's strongest channel is the one that glows onto the person.
    let main = if kg >= kr && kg >= kb {
        1
    } else if kr >= kb {
        0
    } else {
        2
    };
    let mut keep = Vec::with_capacity(pic.pixels().len() / 4);
    for p in pic.pixels_mut().as_chunks_mut::<4>().0 {
        let [r, g, b] = [p[0], p[1], p[2]].map(f32::from);
        let (cb, cr) = chroma(r, g, b);
        let d = ((cb - kcb).powi(2) + (cr - kcr).powi(2)).sqrt();
        let a = ((d - inner) / (outer - inner)).clamp(0.0, 1.0);
        keep.push((a * 255.0).round() as u8);
        // The glow: near the key's colour its channel is held down to the others'.
        let near = (1.0 - (d - inner) / (outer + 50.0 - inner)).clamp(0.0, 1.0);
        if near > 0.0 {
            let others = match main {
                0 => p[1].max(p[2]),
                1 => p[0].max(p[2]),
                _ => p[0].max(p[1]),
            };
            if let Some(c) = p.get_mut(main)
                && *c > others
            {
                *c = (f32::from(*c) - f32::from(*c - others) * near).round() as u8;
            }
        }
    }
    keep
}

/// The camera's background as the model found it, at a small size: its colours where it was seen,
/// and where the person always was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plate {
    pub width: u32,
    pub height: u32,
    /// RGB, three bytes a pixel.
    rgb: Vec<u8>,
    /// Per pixel: 0 never seen uncovered, else 255.
    seen: Vec<u8>,
    /// Per pixel: how much the model said it was the person, at most, in the pictures looked at.
    person: Vec<u8>,
}

/// The plate's width: small, so keying a picture is quick and the camera's noise is smoothed.
const PLATE_W: u32 = 160;
/// How many of the camera's pictures the model looks at.
pub const LOOKS: usize = 4;

impl Plate {
    fn pixels(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// As the recording file keeps it.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(8 + self.pixels() * 5);
        b.extend_from_slice(&self.width.to_le_bytes());
        b.extend_from_slice(&self.height.to_le_bytes());
        b.extend_from_slice(&self.rgb);
        b.extend_from_slice(&self.seen);
        b.extend_from_slice(&self.person);
        b
    }

    /// Read from a recording file; none if it isn't one.
    pub fn from_bytes(b: &[u8]) -> Option<Plate> {
        let width = u32::from_le_bytes(b.get(..4)?.try_into().ok()?);
        let height = u32::from_le_bytes(b.get(4..8)?.try_into().ok()?);
        if width == 0 || height == 0 || width > 1024 || height > 1024 {
            return None;
        }
        let n = width as usize * height as usize;
        if b.len() != 8 + n * 5 {
            return None;
        }
        let rgb = b.get(8..8 + n * 3)?.to_vec();
        let seen = b.get(8 + n * 3..8 + n * 4)?.to_vec();
        let person = b.get(8 + n * 4..)?.to_vec();
        Some(Plate { width, height, rgb, seen, person })
    }

    /// Flipped left to right, for a camera shown mirrored.
    pub fn flipped(&self) -> Plate {
        let w = (self.width as usize).max(1);
        let flip = |v: &[u8], n: usize| -> Vec<u8> {
            let mut out = Vec::with_capacity(v.len());
            for row in v.chunks(w * n) {
                for px in row.chunks(n).rev() {
                    out.extend_from_slice(px);
                }
            }
            out
        };
        Plate { width: self.width, height: self.height, rgb: flip(&self.rgb, 3), seen: flip(&self.seen, 1), person: flip(&self.person, 1) }
    }
}

/// The plate's size for pictures of `w × h`.
fn plate_size(w: u32, h: u32) -> (u32, u32) {
    let pw = PLATE_W.min(w.max(1));
    let ph = ((u64::from(h.max(1)) * u64::from(pw) / u64::from(w.max(1))) as u32).clamp(1, 1024);
    (pw, ph)
}

/// The plate learnt from some of the camera's pictures and the model's mattes of them (each the
/// picture's size): the background's colours where the mattes say background.
pub fn learn(frames: &[(Raster, Matte)]) -> Option<Plate> {
    let (first, _) = frames.first()?;
    let (w, h) = plate_size(first.width(), first.height());
    let n = w as usize * h as usize;
    let mut sum = vec![0u32; n * 3];
    let mut count = vec![0u32; n];
    let mut person = vec![0u8; n];
    for (pic, matte) in frames.iter().take(16) {
        let Some(small) = crate::compose::scale(pic, w, h) else { continue };
        let Ok(m) = matte.resized(w, h) else { continue };
        for (i, (px, a)) in small.pixels().as_chunks::<4>().0.iter().zip(m.alpha()).enumerate() {
            if let Some(p) = person.get_mut(i) {
                *p = (*p).max(*a);
            }
            if *a < 48 {
                if let Some(c) = count.get_mut(i) {
                    *c += 1;
                }
                for (k, v) in px.iter().take(3).enumerate() {
                    if let Some(s) = sum.get_mut(i * 3 + k) {
                        *s += u32::from(*v);
                    }
                }
            }
        }
    }
    let rgb = (0..n * 3).map(|j| sum.get(j).copied().unwrap_or(0).checked_div(count.get(j / 3).copied().unwrap_or(0)).unwrap_or(0) as u8).collect();
    let seen = count.iter().map(|c| if *c > 0 { 255 } else { 0 }).collect();
    Some(Plate { width: w, height: h, rgb, seen, person })
}

/// How much of each pixel of `pic` is kept: what differs from the background the plate saw, and
/// where it never saw any (the person was always there). Smooth at the edges; `pic`'s size.
pub fn plate_key(pic: &Raster, plate: &Plate) -> Vec<u8> {
    let (w, h) = (plate.width, plate.height);
    let full = vec![255u8; pic.pixels().len() / 4];
    let Some(small) = crate::compose::scale(pic, w, h) else { return full };
    let n = plate.pixels();
    let mut a = vec![0f32; n];
    for (i, px) in small.pixels().as_chunks::<4>().0.iter().enumerate().take(n) {
        let seen = plate.seen.get(i).copied().unwrap_or(0) > 0;
        let kept = if seen {
            let d = px.iter().take(3).enumerate().map(|(k, v)| v.abs_diff(plate.rgb.get(i * 3 + k).copied().unwrap_or(0))).max().unwrap_or(0);
            ((f32::from(d) - 22.0) / 26.0).clamp(0.0, 1.0)
        } else {
            f32::from(plate.person.get(i).copied().unwrap_or(255)).max(200.0) / 255.0
        };
        if let Some(v) = a.get_mut(i) {
            *v = kept;
        }
    }
    // Smoothed over its neighbours, so the camera's noise doesn't speckle the edge.
    let (wu, hu) = (w as usize, h as usize);
    let mut soft = vec![0u8; n];
    for y in 0..hu {
        for x in 0..wu {
            let (mut s, mut k) = (0.0, 0.0);
            for dy in y.saturating_sub(1)..(y + 2).min(hu) {
                for dx in x.saturating_sub(1)..(x + 2).min(wu) {
                    s += a.get(dy * wu + dx).copied().unwrap_or(0.0);
                    k += 1.0;
                }
            }
            if let Some(o) = soft.get_mut(y * wu + x) {
                *o = (s / f32::max(k, 1.0) * 255.0).round() as u8;
            }
        }
    }
    Matte::new(w, h, soft).and_then(|m| m.resized(pic.width(), pic.height())).map(|m| m.alpha().to_vec()).unwrap_or(full)
}

/// Learns the camera's background from [`LOOKS`] of its pictures spread through the recording,
/// running the background model on each. `progress` hears how far it has got (0 to 1) and answers
/// whether to go on.
pub fn learn_from(
    rec: &crate::recording::Recording,
    stream: u8,
    model: &dyn Matting,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<Plate, String> {
    let frames = rec.frames(stream);
    if frames.is_empty() {
        return Err("the camera has no pictures".to_string());
    }
    let picks: Vec<usize> = (0..LOOKS).map(|k| (frames.len() - 1) * (2 * k + 1) / (2 * LOOKS)).collect();
    let mut looked = Vec::with_capacity(LOOKS);
    for (k, i) in picks.iter().enumerate() {
        let Some(f) = frames.get(*i) else { continue };
        let Ok(jpeg) = rec.jpeg(f) else { continue };
        let Ok(pic) = screengrabcraft_render::decode(&jpeg) else { continue };
        let base = k as f32 / LOOKS as f32;
        let matte = model.matte(&pic, &mut |f| progress(base + f.clamp(0.0, 1.0) / LOOKS as f32)).map_err(|e| e.to_string())?;
        looked.push((pic, matte));
    }
    learn(&looked).ok_or_else(|| "the camera's pictures couldn't be read".to_string())
}

#[cfg(test)]
mod tests {
    use screengrabcraft_model::Color;

    use super::*;

    /// A green picture with a skin-coloured box (the person) at `x`.
    fn green_with_person(x: u32) -> Raster {
        let mut r = Raster::filled(80, 60, Color::rgb(GREEN[0], GREEN[1], GREEN[2])).unwrap();
        for y in 15..60 {
            for xx in x..x + 20 {
                let i = ((y * 80 + xx) * 4) as usize;
                r.pixels_mut()[i..i + 3].copy_from_slice(&[0xD0, 0x9A, 0x7C]);
            }
        }
        r
    }

    #[test]
    fn a_green_screen_goes_and_the_person_stays() {
        let mut pic = green_with_person(30);
        let keep = colour_key(&mut pic, GREEN, 0.35);
        assert_eq!(keep.len(), 80 * 60);
        assert_eq!(keep[5 * 80 + 5], 0, "the green goes");
        assert_eq!(keep[40 * 80 + 40], 255, "the person stays");
        // A darker, shadowed green goes too.
        let mut shade = Raster::filled(4, 4, Color::rgb(0x00, 0x70, 0x28)).unwrap();
        assert!(colour_key(&mut shade, GREEN, 0.35).iter().all(|a| *a < 40));
        // Its glow on a greenish edge is taken out.
        let mut edge = Raster::filled(1, 1, Color::rgb(0x80, 0xC0, 0x70)).unwrap();
        let _ = colour_key(&mut edge, GREEN, 0.35);
        let p = edge.pixel(0, 0).unwrap();
        assert!(p.g < 0xC0, "{p:?}");
        // Junk tolerance; any picture.
        let _ = colour_key(&mut green_with_person(0), GREEN, f32::NAN);
        let _ = colour_key(&mut Raster::filled(1, 1, Color::BLACK).unwrap(), [255, 0, 255], 2.0);
    }

    /// A room (a gradient) with a dark person-shaped box at `x`, and what a model would say of it.
    fn room(x: u32) -> (Raster, Matte) {
        let mut r = Raster::filled(320, 240, Color::WHITE).unwrap();
        let mut alpha = vec![0u8; 320 * 240];
        for y in 0..240u32 {
            for xx in 0..320u32 {
                let i = ((y * 320 + xx) * 4) as usize;
                let person = (x..x + 80).contains(&xx) && y >= 60;
                let c = if person { [0x30, 0x28, 0x50] } else { [(xx / 2) as u8 + 40, (y / 2) as u8 + 60, 150] };
                r.pixels_mut()[i..i + 3].copy_from_slice(&c);
                if person {
                    alpha[(y * 320 + xx) as usize] = 255;
                }
            }
        }
        (r, Matte::new(320, 240, alpha).unwrap())
    }

    #[test]
    fn the_background_is_learnt_from_a_few_pictures_and_follows_the_person() {
        // Seen with the person at the left, then at the right: the whole room is seen uncovered.
        let plate = learn(&[room(20), room(220)]).unwrap();
        assert_eq!((plate.width, plate.height), (160, 120));
        // The person somewhere new: kept there, the room around them not.
        let (now, _) = room(120);
        let keep = plate_key(&now, &plate);
        assert_eq!(keep.len(), 320 * 240);
        assert!(keep[150 * 320 + 160] > 230, "the person: {}", keep[150 * 320 + 160]);
        assert!(keep[150 * 320 + 40] < 25, "the room: {}", keep[150 * 320 + 40]);
        assert!(keep[20 * 320 + 160] < 25, "above them: {}", keep[20 * 320 + 160]);
        // Where the person always was, they're kept.
        let always = learn(&[room(120)]).unwrap();
        assert!(plate_key(&now, &always)[150 * 320 + 160] > 200);
        // Kept in the file and read back; junk isn't a plate.
        assert_eq!(Plate::from_bytes(&plate.to_bytes()), Some(plate.clone()));
        assert_eq!(Plate::from_bytes(&[1, 0, 0, 0, 1, 0, 0, 0, 9]), None);
        assert_eq!(Plate::from_bytes(&[0xFF; 8]), None);
        assert!(learn(&[]).is_none());
        // Any picture keyed against it never panics.
        assert_eq!(plate_key(&Raster::filled(1, 1, Color::BLACK).unwrap(), &plate).len(), 1);
    }

    #[test]
    fn a_mirrored_camera_is_keyed_against_the_background_mirrored() {
        // The room learnt as recorded; the camera shown mirrored (the person now at the left).
        let plate = learn(&[room(20), room(220)]).unwrap();
        let (now, _) = room(200);
        let shown = screengrabcraft_render::effects::flip(&now, true).unwrap();
        let keep = plate_key(&shown, &plate.flipped());
        assert!(keep[150 * 320 + 80] > 230, "the person: {}", keep[150 * 320 + 80]);
        assert!(keep[150 * 320 + 280] < 25, "the room: {}", keep[150 * 320 + 280]);
        assert!(keep[20 * 320 + 80] < 25, "above them: {}", keep[20 * 320 + 80]);
        // Flipped back it's as it was; a plate read from junk can't make it panic.
        assert_eq!(plate.flipped().flipped(), plate);
        let odd = Plate { width: 0, height: 3, rgb: vec![1; 7], seen: vec![2; 2], person: Vec::new() };
        assert_eq!(odd.flipped().rgb.len(), 7);
    }
}
