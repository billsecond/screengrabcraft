//! X11, through x11rb's own connection (the protocol spoken in Rust, no libxcb).
//!
//! - Monitors: RandR 1.5 `GetMonitors` (names and the primary one); Xinerama's screens when it
//!   has more; else the root window.
//! - Pixels: `GetImage` of the root window in `ZPixmap` format, monitor by monitor, in strips
//!   that stay within the server's request limit, read through the screen's TrueColor visual.
//! - Windows: the window manager's `_NET_CLIENT_LIST_STACKING` (EWMH) with each window's frame
//!   (`_NET_FRAME_EXTENTS`, less a client-side-decorated window's shadow, `_GTK_FRAME_EXTENTS`);
//!   without such a window manager, the root window's mapped children.
//!
//! Every reply comes from another program: values are checked, never indexed blindly.

use std::fmt::Display;

use screengrabcraft_model::raster::checked_len;
use screengrabcraft_model::{IRect, Raster};
use x11rb::connection::{Connection as _, RequestConnection as _};
use x11rb::cookie::Cookie;
use x11rb::protocol::randr::{self, ConnectionExt as _};
use x11rb::protocol::xinerama::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, GetGeometryReply, GetPropertyReply, GetWindowAttributesReply, ImageFormat, ImageOrder, MapState, Screen,
    Setup, TranslateCoordinatesReply, VisualClass, Visualid, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;

use crate::decode::{self, PixelFormat};
use crate::{CaptureError, Frozen, Monitor, WindowInfo};

/// The most monitors and windows taken from the server's lists.
const MAX_MONITORS: usize = 64;
const MAX_WINDOWS: usize = 4096;
/// Windows asked about at once (each is a dozen requests).
const WINDOW_BATCH: usize = 64;
/// Image strips asked for at once.
const STRIP_BATCH: usize = 8;

fn failed(e: impl Display) -> CaptureError {
    CaptureError::Failed(format!("X11: {e}"))
}

fn connect() -> Result<(RustConnection, usize), CaptureError> {
    let (conn, screen) = x11rb::connect(None).map_err(|e| CaptureError::Failed(format!("could not connect to the X server: {e}")))?;
    if conn.setup().roots.get(screen).is_none() {
        return Err(failed(format!("the X server has no screen {screen}")));
    }
    Ok((conn, screen))
}

fn screen_of(conn: &RustConnection, n: usize) -> Result<&Screen, CaptureError> {
    conn.setup().roots.get(n).ok_or_else(|| failed(format!("the X server has no screen {n}")))
}

pub fn monitors() -> Result<Vec<Monitor>, CaptureError> {
    let (conn, n) = connect()?;
    list_monitors(&conn, screen_of(&conn, n)?)
}

pub fn windows() -> Result<Vec<WindowInfo>, CaptureError> {
    let (conn, n) = connect()?;
    list_windows(&conn, screen_of(&conn, n)?)
}

pub fn freeze() -> Result<Frozen, CaptureError> {
    let (conn, n) = connect()?;
    let screen = screen_of(&conn, n)?;
    let pointer = query_pointer(&conn, screen.root);
    let mut monitors = Vec::new();
    let mut first_error = None;
    for m in list_monitors(&conn, screen)? {
        match capture(&conn, screen, m.rect) {
            Ok(pic) => monitors.push((m, pic)),
            Err(e) => {
                log::warn!("monitor {} could not be captured: {e}", m.name);
                first_error.get_or_insert(e);
            }
        }
    }
    if monitors.is_empty() {
        return Err(first_error.unwrap_or(CaptureError::NoMonitor));
    }
    let windows = list_windows(&conn, screen).unwrap_or_else(|e| {
        log::warn!("listing windows failed: {e}");
        Vec::new()
    });
    Ok(Frozen { monitors, windows, pointer })
}

pub fn grab_monitor(id: u32) -> Result<(Monitor, Raster), CaptureError> {
    let (conn, n) = connect()?;
    let screen = screen_of(&conn, n)?;
    let monitor = list_monitors(&conn, screen)?.into_iter().find(|m| m.id == id).ok_or(CaptureError::NoMonitor)?;
    let pic = capture(&conn, screen, monitor.rect)?;
    Ok((monitor, pic))
}

pub fn pointer() -> Option<(i32, i32)> {
    let (conn, n) = connect().ok()?;
    query_pointer(&conn, screen_of(&conn, n).ok()?.root)
}

fn query_pointer(conn: &RustConnection, root: Window) -> Option<(i32, i32)> {
    let reply = conn.query_pointer(root).ok()?.reply().ok()?;
    // On another X screen the coordinates are that screen's.
    reply.same_screen.then_some((i32::from(reply.root_x), i32::from(reply.root_y)))
}

// ---- Monitors ----

/// The monitors, clipped to the screen: RandR's; Xinerama's screens where it has more (a
/// server joining several X screens into one, where RandR sees only the first); else the whole
/// screen as one.
fn list_monitors(conn: &RustConnection, screen: &Screen) -> Result<Vec<Monitor>, CaptureError> {
    let whole = IRect::new(0, 0, u32::from(screen.width_in_pixels), u32::from(screen.height_in_pixels));
    let on_screen = |list: Vec<Monitor>| -> Vec<Monitor> {
        list.into_iter()
            .filter_map(|mut m| {
                m.rect = m.rect.intersect(whole)?;
                Some(m)
            })
            .collect()
    };
    let randr = randr_monitors(conn, screen.root).map(on_screen).unwrap_or_else(|e| {
        log::debug!("RandR monitors unavailable: {e}");
        Vec::new()
    });
    let heads = xinerama_heads(conn).map(on_screen).unwrap_or_else(|e| {
        log::debug!("no Xinerama screens: {e}");
        Vec::new()
    });
    if heads.len() > randr.len() {
        log::debug!("Xinerama lists {} screens and RandR {}; taking Xinerama's", heads.len(), randr.len());
        return Ok(heads);
    }
    if !randr.is_empty() {
        return Ok(randr);
    }
    log::debug!("no monitors listed; taking the whole screen as one");
    if whole.is_empty() {
        return Err(CaptureError::NoMonitor);
    }
    Ok(vec![Monitor { id: 0, name: "Screen".to_string(), rect: whole, scale: 1.0, primary: true }])
}

fn xinerama_heads(conn: &RustConnection) -> Result<Vec<Monitor>, String> {
    let text = |e: &dyn Display| e.to_string();
    if conn.extension_information(xinerama::X11_EXTENSION_NAME).map_err(|e| text(&e))?.is_none() {
        return Err("the X server has no Xinerama".to_string());
    }
    if conn.xinerama_is_active().map_err(|e| text(&e))?.reply().map_err(|e| text(&e))?.state == 0 {
        return Err("Xinerama is off".to_string());
    }
    let reply = conn.xinerama_query_screens().map_err(|e| text(&e))?.reply().map_err(|e| text(&e))?;
    Ok(reply
        .screen_info
        .iter()
        .take(MAX_MONITORS)
        .zip(1u32..)
        .map(|(s, n)| Monitor {
            id: n,
            name: format!("Screen {n}"),
            rect: IRect::new(i32::from(s.x_org), i32::from(s.y_org), u32::from(s.width), u32::from(s.height)),
            scale: 1.0,
            primary: n == 1,
        })
        .collect())
}

fn randr_monitors(conn: &RustConnection, root: Window) -> Result<Vec<Monitor>, String> {
    let text = |e: &dyn Display| e.to_string();
    if conn.extension_information(randr::X11_EXTENSION_NAME).map_err(|e| text(&e))?.is_none() {
        return Err("the X server has no RandR".to_string());
    }
    let version = conn.randr_query_version(1, 5).map_err(|e| text(&e))?.reply().map_err(|e| text(&e))?;
    if (version.major_version, version.minor_version) < (1, 5) {
        return Err(format!("RandR {}.{} lists no monitors (1.5 does)", version.major_version, version.minor_version));
    }
    let reply = conn.randr_get_monitors(root, true).map_err(|e| text(&e))?.reply().map_err(|e| text(&e))?;
    let found: Vec<_> = reply.monitors.iter().take(MAX_MONITORS).collect();
    let names = found.iter().map(|m| conn.get_atom_name(m.name)).collect::<Result<Vec<_>, _>>().map_err(|e| text(&e))?;
    let mut out = Vec::with_capacity(found.len());
    for (i, (m, name)) in found.into_iter().zip(names).enumerate() {
        let name = name.reply().map(|n| decode::text(&n.name, true)).unwrap_or_default();
        let index = u32::try_from(i).unwrap_or(0);
        out.push(Monitor {
            // The name's atom: the server's own, for as long as it runs. Never 0 in practice.
            id: if m.name != 0 { m.name } else { 0x8000_0000 | index },
            name: if name.is_empty() { format!("Monitor {}", index + 1) } else { name },
            rect: IRect::new(i32::from(m.x), i32::from(m.y), u32::from(m.width), u32::from(m.height)),
            scale: 1.0,
            primary: m.primary,
        });
    }
    Ok(out)
}

// ---- Pixels ----

/// How the server lays out images of `depth` drawn through `visual`.
fn pixel_format(setup: &Setup, screen: &Screen, depth: u8, visual: Visualid) -> Result<PixelFormat, CaptureError> {
    let format = setup
        .pixmap_formats
        .iter()
        .find(|f| f.depth == depth)
        .ok_or_else(|| failed(format!("the X server has no image format for depth {depth}")))?;
    let vt = screen
        .allowed_depths
        .iter()
        .flat_map(|d| d.visuals.iter())
        .find(|v| v.visual_id == visual)
        .ok_or_else(|| failed(format!("the screen has no visual {visual:#x}")))?;
    if vt.class != VisualClass::TRUE_COLOR {
        return Err(failed(format!(
            "the screen's colours are of visual class {} (depth {depth}), not TrueColor, which is all ScreenGrabCraft reads",
            u8::from(vt.class)
        )));
    }
    PixelFormat::new(
        format.bits_per_pixel,
        format.scanline_pad,
        setup.image_byte_order == ImageOrder::MSB_FIRST,
        vt.red_mask,
        vt.green_mask,
        vt.blue_mask,
    )
    .map_err(|e| failed(format!("the screen's pixels (depth {depth}) have {e}, which ScreenGrabCraft can't read")))
}

/// A picture of `rect` (root coordinates, on the screen) as RGBA.
fn capture(conn: &RustConnection, screen: &Screen, rect: IRect) -> Result<Raster, CaptureError> {
    let len = checked_len(rect.w, rect.h)?;
    let x = i16::try_from(rect.x).map_err(failed)?;
    let w = u16::try_from(rect.w).map_err(failed)?;
    let setup = conn.setup();
    let format = pixel_format(setup, screen, screen.root_depth, screen.root_visual)?;
    let stride = format.stride(rect.w).ok_or_else(|| failed("the screen is too wide"))?;
    // The server's limit is on requests, but a huge reply can strain it as much; strips that
    // stay within it (BIG-REQUESTS when it has it, 256 KiB when not) are safe everywhere.
    let strips = decode::strips(rect.h, stride, conn.maximum_request_bytes());
    let mut out = Vec::with_capacity(len);
    for batch in strips.chunks(STRIP_BATCH) {
        let mut cookies = Vec::with_capacity(batch.len());
        for &(dy, rows) in batch {
            let y = i16::try_from(i64::from(rect.y) + i64::from(dy)).map_err(failed)?;
            let h = u16::try_from(rows).map_err(failed)?;
            cookies.push((rows, conn.get_image(ImageFormat::Z_PIXMAP, screen.root, x, y, w, h, !0).map_err(failed)?));
        }
        for (rows, cookie) in cookies {
            let reply = cookie.reply().map_err(failed)?;
            let same = reply.depth == screen.root_depth && (reply.visual == screen.root_visual || reply.visual == 0);
            let fmt = if same { format } else { pixel_format(setup, screen, reply.depth, reply.visual)? };
            fmt.convert(&reply.data, rect.w, rows, &mut out).map_err(failed)?;
        }
    }
    Ok(Raster::new(rect.w, rect.h, out)?)
}

// ---- Windows ----

const ATOM_NAMES: [&str; 12] = [
    "_NET_CLIENT_LIST_STACKING",
    "_NET_WM_STATE",
    "_NET_WM_STATE_HIDDEN",
    "_NET_WM_WINDOW_TYPE",
    "_NET_WM_WINDOW_TYPE_DESKTOP",
    "_NET_WM_WINDOW_TYPE_DOCK",
    "_NET_WM_PID",
    "_NET_FRAME_EXTENTS",
    "_GTK_FRAME_EXTENTS",
    "_NET_WM_NAME",
    "UTF8_STRING",
    "WM_STATE",
];

struct Atoms {
    client_list_stacking: Atom,
    state: Atom,
    hidden: Atom,
    window_type: Atom,
    type_desktop: Atom,
    type_dock: Atom,
    pid: Atom,
    frame_extents: Atom,
    gtk_frame_extents: Atom,
    net_name: Atom,
    utf8: Atom,
    wm_state: Atom,
}

fn atoms(conn: &RustConnection) -> Result<Atoms, CaptureError> {
    let cookies = ATOM_NAMES.iter().map(|n| conn.intern_atom(false, n.as_bytes())).collect::<Result<Vec<_>, _>>().map_err(failed)?;
    let mut got = [0; ATOM_NAMES.len()];
    for (slot, cookie) in got.iter_mut().zip(cookies) {
        *slot = cookie.reply().map_err(failed)?.atom;
    }
    let [client_list_stacking, state, hidden, window_type, type_desktop, type_dock, pid, frame_extents, gtk_frame_extents, net_name, utf8, wm_state] =
        got;
    Ok(Atoms {
        client_list_stacking,
        state,
        hidden,
        window_type,
        type_desktop,
        type_dock,
        pid,
        frame_extents,
        gtk_frame_extents,
        net_name,
        utf8,
        wm_state,
    })
}

type Pending<'c, R> = Cookie<'c, RustConnection, R>;

/// The requests about one window, sent together and answered later.
struct Asked<'c> {
    client: Window,
    /// The frame's extents belong to the client window it was asked about.
    own_frame: bool,
    attrs: Pending<'c, GetWindowAttributesReply>,
    geometry: Pending<'c, GetGeometryReply>,
    origin: Pending<'c, TranslateCoordinatesReply>,
    state: Pending<'c, GetPropertyReply>,
    kind: Pending<'c, GetPropertyReply>,
    pid: Pending<'c, GetPropertyReply>,
    frame: Pending<'c, GetPropertyReply>,
    gtk_frame: Pending<'c, GetPropertyReply>,
    net_name: Pending<'c, GetPropertyReply>,
    name: Pending<'c, GetPropertyReply>,
    class: Pending<'c, GetPropertyReply>,
}

fn values32(reply: &GetPropertyReply) -> Vec<u32> {
    reply.value32().map(|v| v.take(MAX_WINDOWS).collect()).unwrap_or_default()
}

/// The visible windows, front first.
fn list_windows(conn: &RustConnection, screen: &Screen) -> Result<Vec<WindowInfo>, CaptureError> {
    let atoms = atoms(conn)?;
    let root = screen.root;
    let longs = u32::try_from(MAX_WINDOWS).unwrap_or(u32::MAX);
    let stacking = conn.get_property(false, root, atoms.client_list_stacking, AtomEnum::WINDOW, 0, longs).map_err(failed)?.reply().map_err(failed)?;
    let stacking = values32(&stacking);
    // (the window whose rectangle is wanted, the client window it shows, whether the client's
    // frame extents apply to that rectangle), front first.
    let pairs: Vec<(Window, Window, bool)> = if stacking.is_empty() {
        log::debug!("no _NET_CLIENT_LIST_STACKING (a window manager without EWMH, or none); listing the root's children");
        top_level_windows(conn, root, atoms.wm_state)?
    } else {
        stacking.iter().rev().map(|&w| (w, w, true)).collect()
    };
    let own = std::process::id();
    let mut out = Vec::new();
    for batch in pairs.chunks(WINDOW_BATCH) {
        let asked =
            batch.iter().map(|&(outer, client, own_frame)| ask(conn, root, &atoms, outer, client, own_frame)).collect::<Result<Vec<_>, _>>()?;
        out.extend(asked.into_iter().filter_map(|a| answer(a, &atoms, own)));
    }
    Ok(out)
}

/// The mapped children of the root window, front first, each with the client window inside it
/// (the child itself without a window manager; under a reparenting one, the descendant with
/// `WM_STATE`).
fn top_level_windows(conn: &RustConnection, root: Window, wm_state: Atom) -> Result<Vec<(Window, Window, bool)>, CaptureError> {
    let tree = conn.query_tree(root).map_err(failed)?.reply().map_err(failed)?;
    let children: Vec<Window> = tree.children.iter().rev().take(MAX_WINDOWS).copied().collect();
    let attrs = children.iter().map(|w| conn.get_window_attributes(*w)).collect::<Result<Vec<_>, _>>().map_err(failed)?;
    let mut out = Vec::new();
    for (w, attrs) in children.into_iter().zip(attrs) {
        let Ok(a) = attrs.reply() else { continue };
        if a.map_state != MapState::VIEWABLE || a.override_redirect || a.class == WindowClass::INPUT_ONLY {
            continue;
        }
        let client = find_client(conn, w, wm_state, 3).unwrap_or(w);
        out.push((w, client, client == w));
    }
    Ok(out)
}

/// The window at or below `w` that has `WM_STATE` (the one the window manager manages).
fn find_client(conn: &RustConnection, w: Window, wm_state: Atom, depth: u32) -> Option<Window> {
    let has = conn.get_property(false, w, wm_state, AtomEnum::ANY, 0, 0).ok()?.reply().ok()?;
    if has.type_ != u32::from(AtomEnum::NONE) {
        return Some(w);
    }
    if depth == 0 {
        return None;
    }
    let tree = conn.query_tree(w).ok()?.reply().ok()?;
    tree.children.iter().rev().take(MAX_WINDOWS).find_map(|c| find_client(conn, *c, wm_state, depth - 1))
}

fn ask<'c>(conn: &'c RustConnection, root: Window, atoms: &Atoms, outer: Window, client: Window, own_frame: bool) -> Result<Asked<'c>, CaptureError> {
    let prop = |p: Atom, t: Atom, longs: u32| conn.get_property(false, client, p, t, 0, longs).map_err(failed);
    let any = u32::from(AtomEnum::ANY);
    let cardinal = u32::from(AtomEnum::CARDINAL);
    let atom = u32::from(AtomEnum::ATOM);
    Ok(Asked {
        client,
        own_frame,
        attrs: conn.get_window_attributes(client).map_err(failed)?,
        geometry: conn.get_geometry(outer).map_err(failed)?,
        origin: conn.translate_coordinates(outer, root, 0, 0).map_err(failed)?,
        state: prop(atoms.state, atom, 64)?,
        kind: prop(atoms.window_type, atom, 64)?,
        pid: prop(atoms.pid, cardinal, 1)?,
        frame: prop(atoms.frame_extents, cardinal, 4)?,
        gtk_frame: prop(atoms.gtk_frame_extents, cardinal, 4)?,
        net_name: prop(atoms.net_name, atoms.utf8, 1024)?,
        name: prop(u32::from(AtomEnum::WM_NAME), any, 1024)?,
        class: prop(u32::from(AtomEnum::WM_CLASS), any, 256)?,
    })
}

/// The window, if it is one to offer: mapped, not minimized, not the desktop or a panel, not
/// this program's, and big enough to point at.
fn answer(a: Asked<'_>, atoms: &Atoms, own_pid: u32) -> Option<WindowInfo> {
    let attrs = a.attrs.reply().ok()?;
    // Unmapped is minimized or on another workspace with most window managers.
    if attrs.map_state != MapState::VIEWABLE || attrs.override_redirect || attrs.class == WindowClass::INPUT_ONLY {
        return None;
    }
    if values32(&a.state.reply().ok()?).contains(&atoms.hidden) {
        return None;
    }
    let kinds = values32(&a.kind.reply().ok()?);
    if kinds.contains(&atoms.type_desktop) || kinds.contains(&atoms.type_dock) {
        return None;
    }
    if values32(&a.pid.reply().ok()?).first() == Some(&own_pid) {
        return None;
    }
    let geometry = a.geometry.reply().ok()?;
    let origin = a.origin.reply().ok()?;
    let (frame, csd) = if a.own_frame {
        (decode::extents(&values32(&a.frame.reply().ok()?)), decode::extents(&values32(&a.gtk_frame.reply().ok()?)))
    } else {
        ([0; 4], [0; 4])
    };
    let rect = decode::frame_rect(
        i32::from(origin.dst_x),
        i32::from(origin.dst_y),
        u32::from(geometry.width),
        u32::from(geometry.height),
        u32::from(geometry.border_width),
        frame,
        csd,
    )?;
    if rect.w < 8 || rect.h < 8 {
        return None;
    }
    let net_name = a.net_name.reply().ok()?;
    let title = if net_name.format == 8 && net_name.type_ == atoms.utf8 && !net_name.value.is_empty() {
        decode::text(&net_name.value, false)
    } else {
        let name = a.name.reply().ok()?;
        if name.format == 8 { decode::text(&name.value, name.type_ == u32::from(AtomEnum::STRING)) } else { String::new() }
    };
    let class = a.class.reply().ok()?;
    let app = if class.format == 8 { decode::wm_class(&class.value) } else { String::new() };
    Some(WindowInfo { id: u64::from(a.client), title, app, rect })
}
