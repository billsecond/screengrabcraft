//! Prints what the capture backend sees (monitors, windows, the pointer) and writes PNGs of
//! each monitor, a fresh grab of one, the whole desktop and the frontmost window.
//!
//! `cargo run -p screengrabcraft-capture --example grab -- [output folder]`

use std::path::{Path, PathBuf};

use screengrabcraft_capture as capture;
use screengrabcraft_model::Raster;

struct Stderr;

impl log::Log for Stderr {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        if record.target().starts_with("screengrabcraft") {
            eprintln!("[{}] {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

fn save(dir: &Path, name: &str, pic: &Raster) -> Result<(), Box<dyn std::error::Error>> {
    let path = dir.join(name);
    image::save_buffer(&path, pic.pixels(), pic.width(), pic.height(), image::ExtendedColorType::Rgba8)?;
    println!("wrote {} ({}×{})", path.display(), pic.width(), pic.height());
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if log::set_logger(&Stderr).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
    let dir = std::env::args_os().nth(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&dir)?;

    println!("backend: {}", capture::backend_name());
    println!("permission: {:?}", capture::permission());
    println!("pointer: {:?}", capture::pointer());
    for m in capture::monitors()? {
        println!("monitor {} {:?} at {:?} scale {} {}", m.id, m.name, m.rect, m.scale, if m.primary { "(primary)" } else { "" });
    }
    for w in capture::windows() {
        println!("window {:#x} {:?} app {:?} at {:?}", w.id, w.title, w.app, w.rect);
    }

    let frozen = capture::freeze()?;
    println!(
        "frozen: {} monitor(s), {} window(s), pointer {:?}, desktop {:?}",
        frozen.monitors.len(),
        frozen.windows.len(),
        frozen.pointer,
        frozen.desktop()
    );
    for (m, pic) in &frozen.monitors {
        save(&dir, &format!("monitor-{}.png", m.id), pic)?;
    }
    if let Some(w) = frozen.windows.first() {
        println!("frontmost window: {:?} at {:?}", w.title, w.rect);
        if let Some(pic) = frozen.crop(w.rect) {
            save(&dir, "front-window.png", &pic)?;
        }
    }
    if let Some((m, _)) = frozen.current_monitor() {
        let (m, pic) = capture::grab_monitor(m.id)?;
        save(&dir, &format!("grab-{}.png", m.id), &pic)?;
    }
    save(&dir, "desktop.png", &capture::grab_area(frozen.desktop())?)?;
    Ok(())
}
