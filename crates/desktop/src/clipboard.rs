//! Pictures to and from the system clipboard.
//!
//! One clipboard handle is kept for the life of the program: on X11 and Wayland a program serves
//! what it copied for as long as its handle lives, so a picture copied by the capture service
//! stays pasteable after the capture window has gone.
//!
//! On Windows only one program at a time can have the clipboard open, and others open it all the
//! time: whatever reads what was just copied (the clipboard's history, Remote Desktop's clipboard,
//! a clipboard manager) holds it for a moment, longer for a big picture or over a network. The
//! system's own call (and arboard's few quick tries) gives up within milliseconds, so a copy here
//! keeps trying for a few seconds ([`BUSY_WAIT`]) before it says it couldn't.

use std::borrow::Cow;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use screengrabcraft_model::Raster;

/// How long a copy keeps trying while another program has the clipboard open (in pauses).
pub const BUSY_WAIT: Duration = Duration::from_secs(4);
/// The same for a paste, which the editor waits for (it's asked for, so less is lost).
const PASTE_WAIT: Duration = Duration::from_secs(1);
/// What the owner is told when the clipboard stayed busy.
pub const BUSY: &str = "Another program kept the clipboard busy";

fn with<T>(f: impl FnOnce(&mut arboard::Clipboard) -> Result<T, arboard::Error>) -> Result<T, arboard::Error> {
    static CLIPBOARD: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();
    let cell = CLIPBOARD.get_or_init(|| Mutex::new(None));
    let mut guard = cell.lock().unwrap_or_else(PoisonError::into_inner);
    if guard.is_none() {
        *guard = Some(arboard::Clipboard::new()?);
    }
    match guard.as_mut() {
        Some(cb) => f(cb),
        None => Err(arboard::Error::Unknown { description: "no clipboard".to_string() }),
    }
}

/// The pause before trying again after the `n`th refusal: short at first (the clipboard is
/// mostly let go within a few milliseconds), then a quarter of a second at most.
fn pause(n: u32) -> Duration {
    Duration::from_millis((10u64 << n.min(5)).min(250))
}

/// `op` tried again while the clipboard is busy, until it isn't or `wait` has gone by in pauses
/// (`sleep` makes them). Any other answer is final. Also how many times it was refused.
fn while_busy<T>(
    wait: Duration,
    mut sleep: impl FnMut(Duration),
    mut op: impl FnMut() -> Result<T, arboard::Error>,
) -> (Result<T, arboard::Error>, u32) {
    let mut waited = Duration::ZERO;
    let mut refused = 0u32;
    loop {
        match op() {
            Err(arboard::Error::ClipboardOccupied) if waited < wait => {
                let p = pause(refused);
                sleep(p);
                waited = waited.saturating_add(p);
                refused = refused.saturating_add(1);
            }
            r => return (r, refused),
        }
    }
}

/// `op` on the clipboard, waiting up to `wait` for it while it's busy; what it did is logged
/// when it had to wait (`what`: what was copied, never its content).
fn when_free<T>(what: &str, wait: Duration, op: impl FnMut() -> Result<T, arboard::Error>) -> Result<T, String> {
    let t = Instant::now();
    let (r, refused) = while_busy(wait, std::thread::sleep, op);
    let ms = t.elapsed().as_millis();
    match &r {
        Ok(_) if refused > 0 => log::info!("clipboard: {what} done after waiting {ms} ms for it ({refused} tries)"),
        Err(arboard::Error::ClipboardOccupied) => log::warn!("clipboard: {what} not done, it stayed busy for {ms} ms ({refused} tries)"),
        Err(e) => log::warn!("clipboard: {what} not done: {e}"),
        Ok(_) => {}
    }
    r.map_err(|e| said(&e))
}

/// An error as the owner is told it.
fn said(e: &arboard::Error) -> String {
    match e {
        arboard::Error::ClipboardOccupied => BUSY.to_string(),
        e => e.to_string(),
    }
}

/// Puts a picture on the clipboard, waiting for it while another program has it open.
pub fn copy_image(r: &Raster) -> Result<(), String> {
    when_free(&format!("copying a {}x{} picture", r.width(), r.height()), BUSY_WAIT, || {
        with(|cb| {
            let image = arboard::ImageData { width: r.width() as usize, height: r.height() as usize, bytes: Cow::Borrowed(r.pixels()) };
            cb.set_image(image)
        })
    })
}

/// The picture on the clipboard, if there is one.
pub fn paste_image() -> Option<Raster> {
    let img = when_free("pasting a picture", PASTE_WAIT, || with(|cb| cb.get_image())).ok()?;
    let (w, h) = (u32::try_from(img.width).ok()?, u32::try_from(img.height).ok()?);
    Raster::new(w, h, img.bytes.into_owned()).ok()
}

/// Puts text on the clipboard (Grab Text's text, a saved file's path).
pub fn copy_text(text: &str) -> Result<(), String> {
    when_free("copying text", BUSY_WAIT, || with(|cb| cb.set_text(text.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn made_up() -> Raster {
        let (w, h) = (64u32, 48u32);
        let mut px = Vec::new();
        for y in 0..h {
            for x in 0..w {
                px.extend_from_slice(&[(x * 4) as u8, (y * 5) as u8, 200, 255]);
            }
        }
        Raster::new(w, h, px).unwrap()
    }

    /// A clipboard another program holds for `busy` tries.
    struct Held {
        busy: u32,
        tries: u32,
        on_it: Option<Raster>,
    }

    impl Held {
        fn set(&mut self, pic: &Raster) -> Result<(), arboard::Error> {
            self.tries += 1;
            if self.tries <= self.busy {
                return Err(arboard::Error::ClipboardOccupied);
            }
            self.on_it = Some(pic.clone());
            Ok(())
        }
    }

    #[test]
    fn a_capture_is_copied_once_another_program_lets_go_of_the_clipboard() {
        // Before, the copy gave up after a few milliseconds and the capture wasn't copied. A
        // clipboard held for about a second and a half now only delays it.
        let pic = made_up();
        let mut cb = Held { busy: 10, tries: 0, on_it: None };
        let mut slept = Duration::ZERO;
        let (r, refused) = while_busy(BUSY_WAIT, |d| slept += d, || cb.set(&pic));
        assert!(r.is_ok());
        assert_eq!(refused, 10);
        assert!(cb.on_it.as_ref() == Some(&pic), "the capture is on the clipboard");
        assert!(slept > Duration::from_secs(1) && slept < BUSY_WAIT, "{slept:?}");
    }

    #[test]
    fn a_clipboard_that_stays_busy_is_given_up_on_in_time() {
        let pic = made_up();
        let mut cb = Held { busy: u32::MAX, tries: 0, on_it: None };
        let mut slept = Duration::ZERO;
        let (r, refused) = while_busy(BUSY_WAIT, |d| slept += d, || cb.set(&pic));
        assert!(matches!(r, Err(arboard::Error::ClipboardOccupied)));
        assert!(cb.on_it.is_none());
        assert!(slept >= BUSY_WAIT && slept <= BUSY_WAIT + Duration::from_millis(250), "{slept:?}");
        assert!(refused < 40, "a bounded number of tries: {refused}");
        assert_eq!(said(&arboard::Error::ClipboardOccupied), BUSY);
    }

    #[test]
    fn only_a_busy_clipboard_is_tried_again() {
        let mut tries = 0;
        let (r, refused) = while_busy(
            BUSY_WAIT,
            |_| {},
            || -> Result<(), arboard::Error> {
                tries += 1;
                Err(arboard::Error::ConversionFailure)
            },
        );
        assert!(matches!(r, Err(arboard::Error::ConversionFailure)));
        assert_eq!((tries, refused), (1, 0));
        // The first pauses are short, none is long.
        assert_eq!(pause(0), Duration::from_millis(10));
        assert_eq!(pause(3), Duration::from_millis(80));
        assert_eq!(pause(30), Duration::from_millis(250));
    }

    /// What [`live_a_picture_is_copied_while_another_program_holds_the_clipboard`] runs in a
    /// process of its own: the clipboard held open for `SGC_HOLD_CLIPBOARD` milliseconds, as
    /// another program holds it, for a window (two that open it for no window share it).
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "run by the live clipboard test"]
    fn live_hold_the_clipboard() {
        let Some(ms) = std::env::var("SGC_HOLD_CLIPBOARD").ok().and_then(|v| v.parse().ok()) else { return };
        let open = winsafe::HWND::GetDesktopWindow().OpenClipboard().unwrap();
        println!("CLIPBOARD-HELD");
        std::thread::sleep(Duration::from_millis(ms));
        drop(open);
    }

    /// The real clipboard, held open by another process for 800 ms while a made-up picture is
    /// copied (`cargo test -p screengrabcraft-desktop live_a_picture -- --ignored`). Before, the
    /// copy was refused.
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "uses this computer's clipboard: leaves a small made-up picture on it"]
    fn live_a_picture_is_copied_while_another_program_holds_the_clipboard() {
        use std::io::BufRead as _;
        let pic = made_up();
        let mut holder = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "clipboard::tests::live_hold_the_clipboard", "--ignored", "--nocapture"])
            .env("SGC_HOLD_CLIPBOARD", "800")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut lines = std::io::BufReader::new(holder.stdout.take().unwrap()).lines();
        assert!(lines.by_ref().map_while(Result::ok).any(|l| l.contains("CLIPBOARD-HELD")), "the other process holds the clipboard");
        // What the system's call alone does: refused while it's held.
        let alone = with(|cb| cb.set_text("refused".to_string()));
        assert!(matches!(alone, Err(arboard::Error::ClipboardOccupied)), "{alone:?}");
        let t = Instant::now();
        copy_image(&pic).unwrap();
        let waited = t.elapsed();
        lines.for_each(drop);
        holder.wait().unwrap();
        assert!(waited >= Duration::from_millis(500), "it waited for the clipboard: {waited:?}");
        let back = paste_image().unwrap();
        assert!(back == pic, "the made-up picture is on the clipboard");
    }
}
