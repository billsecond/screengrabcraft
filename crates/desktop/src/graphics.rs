//! Windows graphics instance configuration, applied before wgpu can initialize any driver
//! (carried over from MailCraft, where #316 and the AMD driver crash below were found).

use eframe::{
    NativeOptions,
    egui_wgpu::WgpuSetup,
    wgpu::{Backends, DeviceType},
};

/// What the start-up probe learned about one DirectX 12 adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dx12Adapter {
    /// `Cpu` is WARP (Microsoft's software rasteriser): it works, but every frame is drawn on the
    /// processor, which makes the app lag on older machines (MailCraft #316).
    pub device_type: DeviceType,
    /// Whether the adapter offers the limits eframe will ask for when it creates the device.
    pub meets_limits: bool,
}

/// The backends to initialize on Windows.
///
/// An explicit `backend_override` (normally `Backends::from_env()`, i.e. `WGPU_BACKEND`) always
/// wins. Otherwise DirectX 12 only, as long as `dx12_adapters` (what a DirectX 12-only probe found)
/// contains a usable hardware GPU. Without one, OpenGL is the way to the GPU: graphics chips whose
/// driver has no DirectX 12 support (Intel 4th generation on Windows 11, MailCraft #316) still have an
/// OpenGL driver, and GL can't reach the AMD driver crash described in [`configure_backends`],
/// because an AMD GPU with a driver shows up as a DirectX 12 adapter and keeps us on DirectX 12.
/// When DirectX 12 offers only WARP, it stays enabled next to GL as a last resort for machines
/// without an OpenGL driver either (virtual machines): wgpu prefers any GPU over a software
/// adapter, so it's only picked when GL has nothing better.
pub fn choose_backends(backend_override: Option<Backends>, dx12_adapters: &[Dx12Adapter]) -> Backends {
    if let Some(backends) = backend_override {
        return backends;
    }
    let hardware = |a: &&Dx12Adapter| a.device_type != DeviceType::Cpu;
    if dx12_adapters.iter().filter(hardware).any(|a| a.meets_limits) {
        Backends::DX12
    } else if !dx12_adapters.is_empty() && !dx12_adapters.iter().any(|a| hardware(&a)) {
        Backends::DX12 | Backends::GL
    } else {
        // No DirectX 12 adapter at all, or only GPUs that can't run the app: GL alone, so a
        // DirectX 12 GPU that would fail device creation isn't picked over the OpenGL one.
        Backends::GL
    }
}

/// Restricts the wgpu instance eframe creates to `backends`.
pub fn configure_backends(options: &mut NativeOptions, backends: Backends) {
    if let WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup {
        // eframe's default includes GL. Creating that backend can crash inside AMD's
        // atio6axx.dll before adapter selection or any Rust error handling runs, so the
        // window never appears. Choosing an adapter afterwards is too late: only leaving GL
        // out of the instance avoids it. This matches the verified WGPU_BACKEND=dx12
        // workaround without mutating process environment variables.
        create.instance_descriptor.backends = backends;
    }
}

/// Chooses the backends ([`choose_backends`]) and applies them to `options`. Without a
/// `backend_override` this first probes DirectX 12 with a short-lived instance of its own, which
/// is dropped before eframe creates the real one.
#[cfg(target_os = "windows")]
pub fn configure(options: &mut NativeOptions, backend_override: Option<Backends>) {
    let backends = match backend_override {
        Some(backends) => {
            log::info!("graphics: backends {backends:?} (WGPU_BACKEND, or OpenGL after a failure)");
            backends
        }
        None => {
            let adapters = probe_dx12(options);
            let backends = choose_backends(None, &adapters);
            log::info!("graphics: {} DirectX 12 adapter(s) {adapters:?}, backends {backends:?}", adapters.len());
            backends
        }
    };
    configure_backends(options, backends);
}

/// Elsewhere wgpu's own choice stands.
#[cfg(not(target_os = "windows"))]
pub fn configure(_options: &mut NativeOptions, _backend_override: Option<Backends>) {}

/// While a window is being opened, this file in the settings folder says so.
const OPENING: &str = "graphics.opening";
/// When this file is in the settings folder, ScreenGrabCraft draws with OpenGL. Deleting it
/// tries DirectX 12 again.
const OPENGL: &str = "graphics.opengl";

/// A record of a window being opened, so that the start after a crash there draws another way.
///
/// Graphics drivers differ, and a window the driver can't make a drawing surface for takes the
/// whole program down inside wgpu (the capture overlay did this under DirectX 12, see
/// `overlay_builder` in the capture service). While a window is being opened `graphics.opening`
/// exists in the settings folder. Should the program die before [`Opening::opened`], the next
/// start finds it ([`fallback_backends`]) and draws with OpenGL from then on. Only on Windows,
/// the one system with a second way to draw; nothing is written elsewhere or without settings.
#[derive(Debug)]
pub struct Opening {
    marker: Option<std::path::PathBuf>,
}

impl Opening {
    pub fn start() -> Opening {
        let dir = if cfg!(target_os = "windows") && !crate::paths::no_prefs() { crate::paths::ensure_settings_dir() } else { None };
        Opening::start_in(dir.as_deref())
    }

    fn start_in(dir: Option<&std::path::Path>) -> Opening {
        let marker = dir.map(|d| d.join(OPENING)).filter(|m| match std::fs::write(m, b"") {
            Ok(()) => true,
            Err(e) => {
                log::warn!("graphics: could not write {}: {e}", m.display());
                false
            }
        });
        Opening { marker }
    }

    /// The window has been drawn (or is gone without trouble). Deliberately not done on drop:
    /// a panic unwinds through the window's owner, and the marker must outlive it.
    pub fn opened(&mut self) {
        if let Some(m) = self.marker.take() {
            let _ = std::fs::remove_file(m);
        }
    }
}

/// Logs the graphics errors wgpu would otherwise end the program with (its default handler
/// panics). A frame drawn wrong is better than a lost capture. wgpu still ends the program when
/// a window's surface can't be made at all; [`Opening`] covers that.
pub fn log_errors(cc: &eframe::CreationContext<'_>) {
    if let Some(rs) = cc.wgpu_render_state.as_ref() {
        rs.device.on_uncaptured_error(std::sync::Arc::new(|e| log::error!("graphics: {e}")));
    }
}

/// The backends to use instead of the usual choice, after a crash while a window was being opened
/// (see [`Opening`]), or `None`.
pub fn fallback_backends() -> Option<Backends> {
    if !cfg!(target_os = "windows") || crate::paths::no_prefs() {
        return None;
    }
    fallback_in(&crate::paths::settings_dir()?)
}

fn fallback_in(dir: &std::path::Path) -> Option<Backends> {
    let opengl = dir.join(OPENGL);
    let opening = dir.join(OPENING);
    if opening.exists() {
        log::warn!("graphics: the last run ended while opening a window; drawing with OpenGL from now on (delete {} to undo)", opengl.display());
        if let Err(e) = std::fs::write(&opengl, b"DirectX 12 failed to open a window; ScreenGrabCraft draws with OpenGL while this file exists.\n") {
            log::warn!("graphics: could not write {}: {e}", opengl.display());
        }
        let _ = std::fs::remove_file(&opening);
        return Some(Backends::GL);
    }
    opengl.exists().then_some(Backends::GL)
}

/// Lists the DirectX 12 adapters, checking each against the limits eframe's device descriptor
/// will require. Empty when this build has no DirectX 12 backend.
#[cfg(target_os = "windows")]
fn probe_dx12(options: &NativeOptions) -> Vec<Dx12Adapter> {
    use eframe::wgpu::{Instance, InstanceDescriptor};
    // `Instance::new` panics when the build has no backend for the platform.
    if !Instance::enabled_backend_features().contains(Backends::DX12) {
        return Vec::new();
    }
    let WgpuSetup::CreateNew(create) = &options.wgpu_options.wgpu_setup else {
        return Vec::new();
    };
    let instance = Instance::new(InstanceDescriptor { backends: Backends::DX12, ..InstanceDescriptor::new_without_display_handle() });
    pollster::block_on(instance.enumerate_adapters(Backends::DX12))
        .iter()
        .map(|adapter| {
            let info = adapter.get_info();
            let required = (create.device_descriptor)(adapter).required_limits;
            let meets_limits = required.check_limits(&adapter.limits());
            log::info!("graphics: DirectX 12 adapter {:?} ({:?}, driver {:?}), limits ok: {meets_limits}", info.name, info.device_type, info.driver);
            Dx12Adapter { device_type: info.device_type, meets_limits }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured_backends(backends: Backends) -> Backends {
        let mut options = NativeOptions::default();
        configure_backends(&mut options, backends);
        let WgpuSetup::CreateNew(create) = options.wgpu_options.wgpu_setup else {
            panic!("default native options must create a wgpu instance");
        };
        create.instance_descriptor.backends
    }

    const fn adapter(device_type: DeviceType, meets_limits: bool) -> Dx12Adapter {
        Dx12Adapter { device_type, meets_limits }
    }

    #[test]
    fn windows_default_initializes_only_dx12() {
        // eframe's own default (PRIMARY | GL) would initialize OpenGL as well.
        let WgpuSetup::CreateNew(eframe_default) = NativeOptions::default().wgpu_options.wgpu_setup else {
            panic!("default native options must create a wgpu instance");
        };
        assert!(eframe_default.instance_descriptor.backends.contains(Backends::GL));
        let gpu = [adapter(DeviceType::IntegratedGpu, true)];
        assert_eq!(configured_backends(choose_backends(None, &gpu)), Backends::DX12);
    }

    #[test]
    fn explicit_backend_override_is_preserved() {
        let warp_only = [adapter(DeviceType::Cpu, true)];
        for backends in [Backends::VULKAN, Backends::GL, Backends::DX12, Backends::VULKAN | Backends::DX12, Backends::empty()] {
            // Whatever the probe would have found, the override wins (the probe doesn't run then).
            assert_eq!(choose_backends(Some(backends), &[]), backends);
            assert_eq!(choose_backends(Some(backends), &warp_only), backends);
            assert_eq!(configured_backends(backends), backends);
        }
    }

    #[test]
    fn gl_when_dx12_has_no_usable_gpu() {
        // MailCraft #316: no DirectX 12 driver, so DirectX 12 offers only WARP (or nothing) and lags.
        let warp_only = [adapter(DeviceType::Cpu, true)];
        assert_eq!(choose_backends(None, &warp_only), Backends::GL | Backends::DX12);
        assert_eq!(choose_backends(None, &[]), Backends::GL);
        // A GPU that can't create eframe's device: GL alone, so it isn't picked over the GL one.
        let weak_gpu = [adapter(DeviceType::IntegratedGpu, false), adapter(DeviceType::Cpu, true)];
        assert_eq!(choose_backends(None, &weak_gpu), Backends::GL);
        // Any usable GPU keeps DirectX 12 only, next to WARP or a weaker GPU.
        let mixed = [adapter(DeviceType::Cpu, true), adapter(DeviceType::IntegratedGpu, false), adapter(DeviceType::DiscreteGpu, true)];
        assert_eq!(choose_backends(None, &mixed), Backends::DX12);
    }

    #[test]
    fn a_crash_while_opening_a_window_switches_to_opengl() {
        let dir = std::env::temp_dir().join(format!("screengrabcraft-graphics-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(fallback_in(&dir), None);
        // A window that opens leaves nothing behind.
        let mut ok = Opening::start_in(Some(&dir));
        assert!(dir.join(OPENING).exists());
        ok.opened();
        assert_eq!(fallback_in(&dir), None);
        // A run that dies first: dropping the record (unwinding) doesn't count as opened.
        drop(Opening::start_in(Some(&dir)));
        assert_eq!(fallback_in(&dir), Some(Backends::GL));
        assert!(!dir.join(OPENING).exists());
        // And it stays OpenGL until the file is deleted.
        assert_eq!(fallback_in(&dir), Some(Backends::GL));
        std::fs::remove_file(dir.join(OPENGL)).unwrap();
        assert_eq!(fallback_in(&dir), None);
        // Without a settings folder nothing is written.
        let mut none = Opening::start_in(None);
        none.opened();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
