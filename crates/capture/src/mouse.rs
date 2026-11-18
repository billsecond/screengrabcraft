//! The mouse pointer, for video recordings: where it is, whether it shows, which of the system's
//! pointers it is (an arrow, a text beam, a hand…), and which buttons are down.
//!
//! On Windows the pointer showing is told from the system's own (each is one shared handle), and
//! its picture is read from the pointer file Windows itself uses for it (named in the registry:
//! the owner's scheme, whatever it is). Those pictures are what was on the owner's screen; they
//! go into the owner's recording only, never into this program. A pointer of a program's own
//! making has no file: it's recorded as the arrow. Elsewhere only the place is known, and the
//! recording draws a plain arrow.
//!
//! Everything read is someone else's: a pointer file is capped, and one that can't be read is
//! no picture, never a panic.

/// The pointer at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PointerState {
    /// Where its hot spot is on the desktop.
    pub x: i32,
    pub y: i32,
    /// Whether it shows (a program playing a video, or typing, may hide it).
    pub visible: bool,
    /// Which pointer it is: a number that stays the same while that pointer shows (0 unknown).
    pub shape: u64,
    pub left: bool,
    pub right: bool,
    pub middle: bool,
}

/// A pointer's picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointerPicture {
    pub width: u32,
    pub height: u32,
    /// `width × height` pixels, RGBA, not premultiplied.
    pub rgba: Vec<u8>,
    /// The pixel of it that points.
    pub hot: (u32, u32),
    /// How wide it's shown on a screen at 100 % (the picture may be larger, to stay sharp).
    pub nominal: u32,
}

/// The largest pointer file read, and the largest picture kept.
#[cfg(target_os = "windows")]
const MAX_FILE: u64 = 4 * 1024 * 1024;
#[cfg(target_os = "windows")]
const MAX_SIDE: u32 = 256;

/// A text beam drawn here, for a system whose own has no file (Windows' default one).
#[cfg(target_os = "windows")]
fn beam() -> PointerPicture {
    let (w, h) = (32u32, 64u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let mut put = |x: u32, y: u32, c: [u8; 4]| {
        if let Some(p) = rgba.get_mut(((y * w + x) * 4) as usize..((y * w + x) * 4 + 4) as usize) {
            p.copy_from_slice(&c);
        }
    };
    // A white edge around a dark beam with a serif at each end, so it shows on anything.
    for pass in [([255u8, 255, 255, 255], 1i32), ([20, 20, 20, 255], 0)] {
        let (c, grow) = pass;
        for y in 8..56i32 {
            for x in (15 - grow)..(17 + grow) {
                put(x as u32, y as u32, c);
            }
        }
        for (y0, y1) in [(6i32, 9i32), (55, 58)] {
            for y in (y0 - grow)..(y1 + grow) {
                for x in (9 - grow)..(23 + grow) {
                    put(x as u32, y as u32, c);
                }
            }
        }
    }
    PointerPicture { width: w, height: h, rgba, hot: (16, 32), nominal: 16 }
}

/// A cross drawn here (the same).
#[cfg(target_os = "windows")]
fn cross() -> PointerPicture {
    let s = 64u32;
    let mut rgba = vec![0u8; (s * s * 4) as usize];
    let mut put = |x: i32, y: i32, c: [u8; 4]| {
        if x >= 0 && y >= 0 && (x as u32) < s && (y as u32) < s {
            let i = ((y as u32 * s + x as u32) * 4) as usize;
            if let Some(p) = rgba.get_mut(i..i + 4) {
                p.copy_from_slice(&c);
            }
        }
    };
    for (c, grow) in [([255u8, 255, 255, 255], 1i32), ([20, 20, 20, 255], 0)] {
        for k in 6..58 {
            // The middle is left open, so what's pointed at is seen.
            if (28..36).contains(&k) {
                continue;
            }
            for d in (31 - grow)..(33 + grow) {
                put(k, d, c);
                put(d, k, c);
            }
        }
    }
    PointerPicture { width: s, height: s, rgba, hot: (32, 32), nominal: 32 }
}

/// A pointer file's picture (`.cur`, or the first picture of an animated `.ani`): the one
/// nearest 64 pixels wide (sharp when enlarged), with its hot spot.
#[cfg(target_os = "windows")]
pub fn read_pointer_file(bytes: &[u8], nominal: u32) -> Option<PointerPicture> {
    // An animated pointer is a RIFF file of pointer pictures: its first "icon" chunk.
    let cur = if bytes.starts_with(b"RIFF") {
        let at = bytes.windows(4).position(|w| w == b"icon")?;
        let size = u32::from_le_bytes(bytes.get(at + 4..at + 8)?.try_into().ok()?) as usize;
        bytes.get(at + 8..(at + 8).checked_add(size)?)?
    } else {
        bytes
    };
    let dir = ico::IconDir::read(std::io::Cursor::new(cur)).ok()?;
    let entries = dir.entries();
    let best = entries
        .iter()
        .filter(|e| e.width() >= 64 && e.width() <= MAX_SIDE && e.height() <= MAX_SIDE)
        .min_by_key(|e| e.width())
        .or_else(|| entries.iter().filter(|e| e.width() <= MAX_SIDE && e.height() <= MAX_SIDE).max_by_key(|e| e.width()))?;
    let hot = best.cursor_hotspot().unwrap_or((0, 0));
    let img = best.decode().ok()?;
    let (width, height) = (img.width(), img.height());
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    let rgba = img.rgba_data().to_vec();
    if rgba.len() != (width * height * 4) as usize {
        return None;
    }
    Some(PointerPicture {
        width,
        height,
        rgba,
        hot: (u32::from(hot.0).min(width - 1), u32::from(hot.1).min(height - 1)),
        nominal: nominal.clamp(16, MAX_SIDE),
    })
}

#[cfg(target_os = "windows")]
mod imp {
    use std::collections::HashMap;

    use winsafe::prelude::*;
    use winsafe::{self as w, co};

    use super::{MAX_FILE, PointerPicture, PointerState, beam, cross, read_pointer_file};

    pub fn state() -> Option<PointerState> {
        let mut ci = w::CURSORINFO::default();
        w::GetCursorInfo(&mut ci).ok()?;
        Some(PointerState {
            x: ci.ptScreenPost.x,
            y: ci.ptScreenPost.y,
            visible: ci.flags.has(co::CURSOR::SHOWING),
            shape: ci.hCursor.ptr() as usize as u64,
            left: w::GetAsyncKeyState(co::VK::LBUTTON),
            right: w::GetAsyncKeyState(co::VK::RBUTTON),
            middle: w::GetAsyncKeyState(co::VK::MBUTTON),
        })
    }

    /// The system's pointers, each by its shared handle, with the name its file goes by in the
    /// registry.
    pub struct Shapes {
        names: HashMap<u64, &'static str>,
    }

    impl Shapes {
        pub fn find() -> Shapes {
            let all = [
                (co::IDC::ARROW, "Arrow"),
                (co::IDC::IBEAM, "IBeam"),
                (co::IDC::WAIT, "Wait"),
                (co::IDC::CROSS, "Crosshair"),
                (co::IDC::UPARROW, "UpArrow"),
                (co::IDC::SIZENWSE, "SizeNWSE"),
                (co::IDC::SIZENESW, "SizeNESW"),
                (co::IDC::SIZEWE, "SizeWE"),
                (co::IDC::SIZENS, "SizeNS"),
                (co::IDC::SIZEALL, "SizeAll"),
                (co::IDC::NO, "No"),
                (co::IDC::HAND, "Hand"),
                (co::IDC::APPSTARTING, "AppStarting"),
                (co::IDC::HELP, "Help"),
                (co::IDC::PIN, "Pin"),
                (co::IDC::PERSON, "Person"),
            ];
            let mut names = HashMap::new();
            for (idc, name) in all {
                // The system's own pointers are shared: the handle is the one every program
                // shows, and it isn't ours to destroy (so it's let go of, not dropped).
                if let Ok(mut shared) = w::HINSTANCE::NULL.LoadCursor(w::IdIdcStr::Idc(idc)) {
                    names.insert(shared.leak().ptr() as usize as u64, name);
                }
            }
            Shapes { names }
        }

        /// The picture of a pointer that is one of the system's; none for any other.
        pub fn picture(&self, shape: u64) -> Option<PointerPicture> {
            let name = *self.names.get(&shape)?;
            let key = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).open_subkey("Control Panel\\Cursors").ok();
            let base = key.as_ref().and_then(|k| k.get_value::<u32, _>("CursorBaseSize").ok()).unwrap_or(32).clamp(16, 256);
            let file: String = key.as_ref().and_then(|k| k.get_value::<String, _>(name).ok()).unwrap_or_default();
            let from_file = (!file.trim().is_empty()).then(|| read(&expand(file.trim()), base)).flatten();
            from_file.or_else(|| match name {
                "IBeam" => Some(PointerPicture { nominal: base / 2, ..beam() }),
                "Crosshair" => Some(PointerPicture { nominal: base, ..cross() }),
                _ => None,
            })
        }
    }

    /// `%SystemRoot%` and the like in a path from the registry.
    fn expand(path: &str) -> String {
        let mut out = String::new();
        let mut rest = path;
        while let Some(a) = rest.find('%') {
            out.push_str(rest.get(..a).unwrap_or_default());
            let after = rest.get(a + 1..).unwrap_or_default();
            match after.find('%') {
                Some(b) => {
                    let var = after.get(..b).unwrap_or_default();
                    out.push_str(&std::env::var(var).unwrap_or_default());
                    rest = after.get(b + 1..).unwrap_or_default();
                }
                None => {
                    rest = after;
                    break;
                }
            }
        }
        out.push_str(rest);
        out
    }

    fn read(path: &str, base: u32) -> Option<PointerPicture> {
        let p = std::path::Path::new(path);
        if std::fs::metadata(p).ok()?.len() > MAX_FILE {
            return None;
        }
        read_pointer_file(&std::fs::read(p).ok()?, base)
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::{PointerPicture, PointerState};

    pub fn state() -> Option<PointerState> {
        let (x, y) = crate::pointer()?;
        Some(PointerState { x, y, visible: true, ..PointerState::default() })
    }

    pub struct Shapes;

    impl Shapes {
        pub fn find() -> Shapes {
            Shapes
        }

        pub fn picture(&self, _shape: u64) -> Option<PointerPicture> {
            None
        }
    }
}

pub use imp::Shapes;

/// The pointer now; none where the system doesn't say.
pub fn state() -> Option<PointerState> {
    imp::state()
}

#[cfg(test)]
#[cfg(target_os = "windows")]
mod tests {
    use super::*;

    /// A pointer file with one 2 × 2 picture (32 bits), its hot spot at (1, 1).
    fn cur() -> Vec<u8> {
        let mut dir = ico::IconDir::new(ico::ResourceType::Cursor);
        let img = ico::IconImage::from_rgba_data(2, 2, vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 0]);
        let mut entry = ico::IconDirEntry::encode(&img).unwrap();
        // (The crate writes a cursor's hot spot from its image's.)
        let mut with_hot = img.clone();
        with_hot.set_cursor_hotspot(Some((1, 1)));
        entry = ico::IconDirEntry::encode(&with_hot).unwrap_or(entry);
        dir.add_entry(entry);
        let mut out = Vec::new();
        dir.write(&mut out).unwrap();
        out
    }

    #[test]
    fn a_pointer_file_gives_its_picture_and_hot_spot() {
        let p = read_pointer_file(&cur(), 32).unwrap();
        assert_eq!((p.width, p.height, p.hot, p.nominal), (2, 2, (1, 1), 32));
        assert_eq!(p.rgba.get(..4), Some(&[255u8, 0, 0, 255][..]));
        // An animated one: the first picture in it.
        let mut ani = b"RIFF\x00\x00\x00\x00ACONicon".to_vec();
        let c = cur();
        ani.extend_from_slice(&(c.len() as u32).to_le_bytes());
        ani.extend_from_slice(&c);
        assert_eq!(read_pointer_file(&ani, 48).map(|p| (p.width, p.nominal)), Some((2, 48)));
    }

    #[test]
    fn junk_is_no_picture_not_a_panic() {
        for junk in [
            Vec::new(),
            b"RIFF".to_vec(),
            b"RIFF\x00\x00\x00\x00ACONicon\xff\xff\xff\xff".to_vec(),
            vec![0u8; 64],
            vec![0xFF; 4096],
            cur()[..10].to_vec(),
        ] {
            let _ = read_pointer_file(&junk, 32);
        }
        assert!(read_pointer_file(&[1, 2, 3], 32).is_none());
    }

    #[test]
    fn the_pointers_drawn_here_have_something_to_show() {
        for p in [beam(), cross()] {
            assert_eq!(p.rgba.len(), (p.width * p.height * 4) as usize);
            assert!(p.rgba.chunks(4).any(|c| c[3] == 255 && c[0] < 40), "a dark line");
            assert!(p.rgba.chunks(4).any(|c| c[3] == 255 && c[0] > 200), "a white edge");
            assert!(p.hot.0 < p.width && p.hot.1 < p.height);
        }
    }

    /// The pointer of this computer, now (Windows): the system's arrow has a picture.
    #[cfg(target_os = "windows")]
    #[test]
    fn the_systems_pointers_are_known() {
        let shapes = Shapes::find();
        // Whatever shows now is somewhere, and asking for an unknown one is none.
        let _ = state();
        assert!(shapes.picture(1).is_none());
    }
}
