//! The desktop's screenshot portal (`org.freedesktop.portal.Screenshot`) over the session bus:
//! the way a Wayland program gets a picture of the screen.
//!
//! The call returns a request object at once; the answer comes later as that object's
//! `Response` signal (after a dialog, on desktops that ask the owner the first time). Per the
//! portal's documentation the signal is subscribed to before the call, at the path the
//! `handle_token` option makes predictable. The answer carries a `file://` URI of a PNG.
//!
//! The waiting is done under `async_io::block_on` with a timer: zbus's blocking signal
//! iterator can't stop waiting by itself.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fmt::Display;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use futures_lite::{StreamExt, future};
use screengrabcraft_model::Raster;
use screengrabcraft_model::raster::{MAX_PIXELS, MAX_SIDE, checked_len};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use crate::{CaptureError, decode};

const DESTINATION: &str = "org.freedesktop.portal.Desktop";
const DESKTOP_PATH: &str = "/org/freedesktop/portal/desktop";
const SCREENSHOT: &str = "org.freedesktop.portal.Screenshot";
const REQUEST: &str = "org.freedesktop.portal.Request";
/// How long the portal (and the owner, if it asks) has to answer.
const TIMEOUT: Duration = Duration::from_secs(60);
/// The largest screenshot file read (a PNG of the largest picture taken in is far smaller).
const MAX_FILE: u64 = 1 << 30;

static TOKENS: AtomicU32 = AtomicU32::new(0);

fn failed(e: impl Display) -> CaptureError {
    CaptureError::Failed(e.to_string())
}

/// A picture of the whole desktop, now.
pub fn screenshot() -> Result<Raster, CaptureError> {
    let started = Instant::now();
    let uri = async_io::block_on(request())?;
    let path = decode::file_uri_path(&uri)
        .map(|bytes| PathBuf::from(OsString::from_vec(bytes)))
        .ok_or_else(|| failed(format!("the screenshot portal answered with {uri:?}, not a local file")))?;
    let pic = read_picture(&path)?;
    log::info!("the screenshot portal gave {}×{} pixels in {} ms ({})", pic.width(), pic.height(), started.elapsed().as_millis(), path.display());
    // The portal's documentation gives the file to no one (GNOME keeps it in the owner's
    // Pictures folder, as any screenshot), so it is left where it is.
    Ok(pic)
}

/// `fut`, unless the deadline comes first.
async fn within<T>(deadline: Instant, fut: impl Future<Output = T>) -> Result<T, CaptureError> {
    future::or(async { Ok(fut.await) }, async {
        async_io::Timer::at(deadline).await;
        Err(failed("the desktop's screenshot portal did not answer within a minute"))
    })
    .await
}

/// Asks the portal for a screenshot and waits for its answer: the picture's URI.
async fn request() -> Result<String, CaptureError> {
    let deadline = Instant::now() + TIMEOUT;
    let conn = within(deadline, zbus::Connection::session()).await?.map_err(|e| failed(format!("the session bus could not be reached: {e}")))?;
    let sender = conn.unique_name().map(|n| decode::portal_sender(n.as_str())).ok_or_else(|| failed("the session bus gave this program no name"))?;
    let token = format!("screengrabcraft_{}_{}", std::process::id(), TOKENS.fetch_add(1, Ordering::Relaxed));
    let expected = format!("{DESKTOP_PATH}/request/{sender}/{token}");
    let mut responses = within(deadline, subscribe(&conn, &expected)).await??;

    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    options.insert("interactive", Value::from(false));
    let reply = within(deadline, conn.call_method(Some(DESTINATION), DESKTOP_PATH, Some(SCREENSHOT), "Screenshot", &("", options)))
        .await?
        .map_err(call_failed)?;
    let handle: OwnedObjectPath = reply.body().deserialize().map_err(|e| failed(format!("the screenshot portal's reply: {e}")))?;
    if handle.as_str() != expected {
        // Portals before 0.9 choose the path themselves.
        log::debug!("the screenshot portal answers at {} rather than {expected}", handle.as_str());
        responses = within(deadline, subscribe(&conn, handle.as_str())).await??;
    }
    match within(deadline, answer(&mut responses)).await {
        Ok(result) => result,
        Err(timeout) => {
            // Take the portal's dialog away, if it still shows one.
            let close = conn.call_method(Some(DESTINATION), handle.as_str(), Some(REQUEST), "Close", &());
            if let Ok(Err(e)) = within(Instant::now() + Duration::from_secs(2), close).await {
                log::debug!("closing the screenshot request failed: {e}");
            }
            Err(timeout)
        }
    }
}

/// The `Response` signals of the request object at `path`.
async fn subscribe(conn: &zbus::Connection, path: &str) -> Result<zbus::MessageStream, CaptureError> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(REQUEST)
        .and_then(|b| b.member("Response"))
        .and_then(|b| b.path(path))
        .map_err(failed)?
        .build();
    zbus::MessageStream::for_match_rule(rule, conn, Some(8))
        .await
        .map_err(|e| failed(format!("the session bus refused to pass on the portal's answer: {e}")))
}

/// The URI from the request's `Response`: code 0 is a picture, 1 the owner said no.
async fn answer(responses: &mut zbus::MessageStream) -> Result<String, CaptureError> {
    let Some(msg) = responses.next().await else {
        return Err(failed("the session bus closed before the screenshot portal answered"));
    };
    let msg = msg.map_err(failed)?;
    let (code, results): (u32, HashMap<String, OwnedValue>) =
        msg.body().deserialize().map_err(|e| failed(format!("the screenshot portal's answer: {e}")))?;
    match code {
        0 => results
            .get("uri")
            .and_then(|v| v.downcast_ref::<&str>().ok().map(str::to_owned))
            .ok_or_else(|| failed("the screenshot portal answered without a picture")),
        1 => Err(CaptureError::Cancelled),
        other => Err(failed(format!("the desktop's screenshot portal could not take the screenshot (response {other})"))),
    }
}

/// A desktop with no screenshot portal can't capture at all; anything else is a failure.
fn call_failed(e: zbus::Error) -> CaptureError {
    if let zbus::Error::MethodError(name, detail, _) = &e {
        let missing = [
            "org.freedesktop.DBus.Error.ServiceUnknown",
            "org.freedesktop.DBus.Error.NameHasNoOwner",
            "org.freedesktop.DBus.Error.UnknownMethod",
            "org.freedesktop.DBus.Error.UnknownInterface",
            "org.freedesktop.DBus.Error.UnknownObject",
        ];
        if missing.contains(&name.as_str()) {
            log::warn!("the desktop has no screenshot portal ({}: {})", name.as_str(), detail.as_deref().unwrap_or_default());
            return CaptureError::Unsupported;
        }
    }
    failed(format!("the desktop's screenshot portal refused: {e}"))
}

/// The portal's PNG as RGBA, within the sizes ScreenGrabCraft takes.
fn read_picture(path: &Path) -> Result<Raster, CaptureError> {
    let unreadable = |e: &dyn Display| failed(format!("the screenshot {} could not be read: {e}", path.display()));
    let size = std::fs::metadata(path).map_err(|e| unreadable(&e))?.len();
    if size > MAX_FILE {
        return Err(unreadable(&format!("it is {size} bytes")));
    }
    let mut reader = image::ImageReader::open(path).map_err(|e| unreadable(&e))?.with_guessed_format().map_err(|e| unreadable(&e))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    // The RGBA picture, and as much again for the decoder's own buffers (16-bit PNGs).
    limits.max_alloc = Some(MAX_PIXELS * 8);
    reader.limits(limits);
    let img = reader.decode().map_err(|e| unreadable(&e))?;
    let (w, h) = (img.width(), img.height());
    checked_len(w, h)?;
    Ok(Raster::new(w, h, img.into_rgba8().into_raw())?)
}
