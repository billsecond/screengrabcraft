//! The Wayland outputs (monitors), for cutting the portal's picture of the whole desktop.
//!
//! Spoken with wayland-client's own backend (Rust, no libwayland). `xdg-output` gives each
//! output's place and size in the desktop's logical coordinates, which is what the portal's
//! picture covers; without it, `wl_output`'s position and its mode divided by its scale stand in.
//! Nothing here sees pixels.

use wayland_client::protocol::wl_output::{self, WlOutput};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_manager_v1::{self, ZxdgOutputManagerV1};
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_v1::{self, ZxdgOutputV1};

use screengrabcraft_model::IRect;

use crate::Monitor;

/// The most outputs taken from the compositor.
const MAX_OUTPUTS: usize = 64;
/// The longest name kept.
const MAX_NAME: usize = 256;

#[derive(Default)]
struct Output {
    /// The compositor's global name for it: the monitor's id.
    global: u32,
    proxy: Option<WlOutput>,
    position: (i32, i32),
    make_model: String,
    /// Rotated a quarter turn either way.
    sideways: bool,
    mode: Option<(i32, i32)>,
    scale: i32,
    name: Option<String>,
    description: Option<String>,
    logical_position: Option<(i32, i32)>,
    logical_size: Option<(i32, i32)>,
    xdg_name: Option<String>,
}

#[derive(Default)]
struct State {
    outputs: Vec<Output>,
    xdg: Option<ZxdgOutputManagerV1>,
}

fn short(mut s: String) -> String {
    if s.len() > MAX_NAME {
        let mut end = MAX_NAME;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
    s
}

impl Dispatch<WlRegistry, ()> for State {
    fn event(state: &mut State, registry: &WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<State>) {
        let wl_registry::Event::Global { name, interface, version } = event else { return };
        if interface == WlOutput::interface().name && state.outputs.len() < MAX_OUTPUTS {
            let index = state.outputs.len();
            let proxy = registry.bind::<WlOutput, _, _>(name, version.min(WlOutput::interface().version).min(4), qh, index);
            state.outputs.push(Output { global: name, proxy: Some(proxy), scale: 1, ..Output::default() });
        } else if interface == ZxdgOutputManagerV1::interface().name && state.xdg.is_none() {
            state.xdg = Some(registry.bind::<ZxdgOutputManagerV1, _, _>(name, version.min(ZxdgOutputManagerV1::interface().version).min(3), qh, ()));
        }
    }
}

impl Dispatch<WlOutput, usize> for State {
    fn event(state: &mut State, _: &WlOutput, event: wl_output::Event, index: &usize, _: &Connection, _: &QueueHandle<State>) {
        let Some(out) = state.outputs.get_mut(*index) else { return };
        match event {
            wl_output::Event::Geometry { x, y, make, model, transform, .. } => {
                out.position = (x, y);
                out.make_model = short(format!("{make} {model}").trim().to_string());
                out.sideways = matches!(
                    transform,
                    WEnum::Value(
                        wl_output::Transform::_90 | wl_output::Transform::_270 | wl_output::Transform::Flipped90 | wl_output::Transform::Flipped270
                    )
                );
            }
            wl_output::Event::Mode { flags: WEnum::Value(flags), width, height, .. } if flags.contains(wl_output::Mode::Current) => {
                out.mode = Some((width, height));
            }
            wl_output::Event::Scale { factor } => out.scale = factor,
            wl_output::Event::Name { name } => out.name = Some(short(name)),
            wl_output::Event::Description { description } => out.description = Some(short(description)),
            _ => {}
        }
    }
}

impl Dispatch<ZxdgOutputManagerV1, ()> for State {
    fn event(_: &mut State, _: &ZxdgOutputManagerV1, _: zxdg_output_manager_v1::Event, _: &(), _: &Connection, _: &QueueHandle<State>) {}
}

impl Dispatch<ZxdgOutputV1, usize> for State {
    fn event(state: &mut State, _: &ZxdgOutputV1, event: zxdg_output_v1::Event, index: &usize, _: &Connection, _: &QueueHandle<State>) {
        let Some(out) = state.outputs.get_mut(*index) else { return };
        match event {
            zxdg_output_v1::Event::LogicalPosition { x, y } => out.logical_position = Some((x, y)),
            zxdg_output_v1::Event::LogicalSize { width, height } => out.logical_size = Some((width, height)),
            zxdg_output_v1::Event::Name { name } => out.xdg_name = Some(short(name)),
            _ => {}
        }
    }
}

/// The outputs as monitors, in logical coordinates; the one at the origin is the primary one
/// (Wayland has no such thing).
pub fn outputs() -> Result<Vec<Monitor>, String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland connection: {e}"))?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let _registry = conn.display().get_registry(&qh, ());
    let mut state = State::default();
    // The globals (binding the outputs), then the outputs' own events.
    queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
    queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
    if let Some(manager) = &state.xdg {
        let asked: Vec<ZxdgOutputV1> =
            state.outputs.iter().enumerate().filter_map(|(i, o)| Some(manager.get_xdg_output(o.proxy.as_ref()?, &qh, i))).collect();
        queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
        for x in asked {
            x.destroy();
        }
    } else {
        log::info!("the compositor has no xdg-output; placing the outputs by wl_output");
    }
    let mut monitors: Vec<Monitor> = state.outputs.iter().enumerate().filter_map(|(i, o)| monitor_of(i, o)).collect();
    let primary = monitors.iter().position(|m| m.rect.contains(0, 0)).unwrap_or(0);
    if let Some(m) = monitors.get_mut(primary) {
        m.primary = true;
    }
    for o in &state.outputs {
        if let Some(p) = &o.proxy
            && p.version() >= 3
        {
            p.release();
        }
    }
    Ok(monitors)
}

fn monitor_of(index: usize, o: &Output) -> Option<Monitor> {
    let scale = o.scale.max(1);
    // The mode in the desktop's orientation.
    let physical = o.mode.map(|(w, h)| if o.sideways { (h, w) } else { (w, h) });
    let (lw, lh) = o.logical_size.or_else(|| physical.map(|(w, h)| (w / scale, h / scale)))?;
    let (x, y) = o.logical_position.unwrap_or(o.position);
    let (w, h) = (u32::try_from(lw).ok().filter(|v| *v > 0)?, u32::try_from(lh).ok().filter(|v| *v > 0)?);
    let pixel_scale = match physical {
        Some((pw, _)) if pw > 0 => pw as f32 / w as f32,
        _ => scale as f32,
    };
    let name = [&o.xdg_name, &o.name, &o.description]
        .into_iter()
        .flatten()
        .find(|n| !n.is_empty())
        .cloned()
        .or_else(|| (!o.make_model.is_empty()).then(|| o.make_model.clone()))
        .unwrap_or_else(|| format!("Output {}", index + 1));
    Some(Monitor { id: o.global, name, rect: IRect::new(x, y, w, h), scale: pixel_scale, primary: false })
}
