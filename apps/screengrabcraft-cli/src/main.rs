//! `screengrabcraft-cli`: ScreenGrabCraft from the command line and from scripts.
//!
//! ```text
//! screengrabcraft-cli capture [--screen N | --all | --area X,Y,W,H | --window TITLE] [--delay S] [-o FILE] [--clipboard]
//! screengrabcraft-cli monitors | windows
//! screengrabcraft-cli render IN [-o OUT]                 a project or picture flattened to a picture
//! screengrabcraft-cli run IN --cmd ID[=JSON]... [-o OUT]  editing commands, as the editor runs them
//! screengrabcraft-cli commands                            every editing command, as JSON
//! screengrabcraft-cli stitch FRAME... -o OUT              join the frames of a scrolling page
//! screengrabcraft-cli service install|uninstall|status    the capture service at sign-in
//! screengrabcraft-cli --version
//! ```
//!
//! Output files take their format from their extension: .png .jpg .webp .bmp .gif .pdf, or
//! .sgcraft for a project with its annotations still editable.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use screengrabcraft_capture as capture;
use screengrabcraft_engine::Session;
use screengrabcraft_model::{Document, IRect, ImageFormat, PROJECT_EXT, Raster};
use screengrabcraft_render::project;
use serde_json::{Value, json};

const USAGE: &str = "usage:
  screengrabcraft-cli capture [--screen N | --all | --area X,Y,W,H | --window TITLE] [--delay S] [-o FILE] [--clipboard]
  screengrabcraft-cli monitors
  screengrabcraft-cli windows
  screengrabcraft-cli render IN [-o OUT]
  screengrabcraft-cli run IN --cmd ID[=JSON] [--cmd ...] [-o OUT]
  screengrabcraft-cli commands
  screengrabcraft-cli stitch FRAME... -o OUT
  screengrabcraft-cli service install|uninstall|status
  screengrabcraft-cli --version

Output formats follow the extension: .png .jpg .webp .bmp .gif .pdf, or .sgcraft (a project).";

type R<T> = Result<T, String>;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("--version" | "-V" | "version") => {
            println!("{}", screengrabcraft_desktop::version::line("screengrabcraft-cli"));
            Ok(())
        }
        Some("capture") => cmd_capture(&args[1..]),
        Some("monitors") => cmd_monitors(),
        Some("windows") => cmd_windows(),
        Some("render") => cmd_render(&args[1..]),
        Some("run") => cmd_run(&args[1..]),
        Some("commands") => {
            println!("{}", serde_json::to_string_pretty(&screengrabcraft_engine::describe()).unwrap_or_default());
            Ok(())
        }
        Some("stitch") => cmd_stitch(&args[1..]),
        Some("service") => cmd_service(args.get(1).map(String::as_str)),
        Some("-h" | "--help" | "help") | None => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The value after `flag`, if given.
fn opt<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn output(args: &[String]) -> Option<PathBuf> {
    opt(args, "-o").or_else(|| opt(args, "--output")).map(PathBuf::from)
}

/// The first argument that isn't a flag or a flag's value.
fn positional(args: &[String], flags_with_values: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
            continue;
        }
        if flags_with_values.contains(&a.as_str()) {
            skip = true;
            continue;
        }
        if !a.starts_with("--") && a != "-o" {
            out.push(a.clone());
        }
    }
    out
}

fn format_of(path: &Path) -> R<Option<ImageFormat>> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext == PROJECT_EXT {
        return Ok(None);
    }
    ImageFormat::from_extension(&ext).map(Some).ok_or_else(|| format!("{}: unknown format `.{ext}`", path.display()))
}

fn write_picture(r: &Raster, path: &Path) -> R<()> {
    let f = format_of(path)?.unwrap_or(ImageFormat::Png);
    let bytes = screengrabcraft_render::encode(r, f, 92).map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn write_doc(doc: &Document, path: &Path) -> R<()> {
    match format_of(path)? {
        None => {
            let bytes = project::save(doc).map_err(|e| e.to_string())?;
            std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
        }
        Some(_) => write_picture(&screengrabcraft_render::render(doc).ok_or("the picture could not be drawn")?, path),
    }
}

fn read_doc(path: &Path) -> R<Document> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if project::sniff(&bytes) {
        let (doc, skipped) = project::load(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        if skipped > 0 {
            eprintln!("warning: {skipped} annotation(s) could not be read");
        }
        Ok(doc)
    } else {
        Ok(Document::new(screengrabcraft_render::decode(&bytes).map_err(|e| format!("{}: {e}", path.display()))?))
    }
}

fn default_name(ext: &str) -> PathBuf {
    let settings = screengrabcraft_desktop::store::load_settings();
    let name = screengrabcraft_model::settings::expand_name(&settings.file_name, screengrabcraft_desktop::time::now(), "", 1);
    let dir = screengrabcraft_desktop::paths::captures_dir(&settings);
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{name}.{ext}"))
}

fn parse_area(text: &str) -> R<IRect> {
    let n: Vec<i64> = text.split(',').map(|p| p.trim().parse::<i64>()).collect::<Result<_, _>>().map_err(|_| "--area is X,Y,W,H".to_string())?;
    match n.as_slice() {
        [x, y, w, h] if *w > 0 && *h > 0 => Ok(IRect::new(
            i32::try_from(*x).map_err(|e| e.to_string())?,
            i32::try_from(*y).map_err(|e| e.to_string())?,
            u32::try_from(*w).map_err(|e| e.to_string())?,
            u32::try_from(*h).map_err(|e| e.to_string())?,
        )),
        _ => Err("--area is X,Y,W,H with a positive width and height".to_string()),
    }
}

fn cmd_capture(args: &[String]) -> R<()> {
    if let Some(d) = opt(args, "--delay") {
        let secs: u64 = d.parse().map_err(|_| "--delay is a number of seconds")?;
        std::thread::sleep(std::time::Duration::from_secs(secs.min(600)));
    }
    if capture::permission() == capture::Permission::Denied {
        capture::request_permission();
        return Err(capture::CaptureError::NotPermitted.to_string());
    }
    let pic = if let Some(area) = opt(args, "--area") {
        capture::grab_area(parse_area(area)?).map_err(|e| e.to_string())?
    } else if let Some(title) = opt(args, "--window") {
        let needle = title.to_lowercase();
        let w = capture::windows()
            .into_iter()
            .find(|w| w.title.to_lowercase().contains(&needle) || w.app.to_lowercase().contains(&needle))
            .ok_or_else(|| format!("no window titled like {title:?} (see `screengrabcraft-cli windows`)"))?;
        capture::grab_area(w.rect).map_err(|e| e.to_string())?
    } else if args.iter().any(|a| a == "--all") {
        let f = capture::freeze().map_err(|e| e.to_string())?;
        f.crop(f.desktop()).ok_or("the screens could not be put together")?
    } else {
        let monitors = capture::monitors().map_err(|e| e.to_string())?;
        let m = match opt(args, "--screen") {
            Some(n) => {
                let i: usize = n.parse().map_err(|_| "--screen is a number from `screengrabcraft-cli monitors`")?;
                monitors.get(i).ok_or_else(|| format!("there is no screen {i}"))?.clone()
            }
            None => monitors.first().ok_or("no monitor")?.clone(),
        };
        capture::grab_monitor(m.id).map_err(|e| e.to_string())?.1
    };
    if args.iter().any(|a| a == "--clipboard") {
        screengrabcraft_desktop::clipboard::copy_image(&pic)?;
        // The clipboard is served while this program lives, on some systems.
        if !cfg!(target_os = "windows") && !cfg!(target_os = "macos") {
            eprintln!("note: on X11 and Wayland the picture stays on the clipboard only while a program holds it");
        }
    }
    if output(args).is_some() || !args.iter().any(|a| a == "--clipboard") {
        let path = output(args).unwrap_or_else(|| default_name("png"));
        write_picture(&pic, &path)?;
        println!("{}", path.display());
    }
    Ok(())
}

fn cmd_monitors() -> R<()> {
    let ms = capture::monitors().map_err(|e| e.to_string())?;
    let v: Vec<Value> = ms
        .iter()
        .enumerate()
        .map(|(i, m)| json!({"index": i, "id": m.id, "name": m.name, "x": m.rect.x, "y": m.rect.y, "width": m.rect.w, "height": m.rect.h, "scale": m.scale, "primary": m.primary}))
        .collect();
    println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    Ok(())
}

fn cmd_windows() -> R<()> {
    let v: Vec<Value> = capture::windows()
        .iter()
        .map(|w| json!({"id": w.id, "title": w.title, "app": w.app, "x": w.rect.x, "y": w.rect.y, "width": w.rect.w, "height": w.rect.h}))
        .collect();
    println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    Ok(())
}

fn cmd_render(args: &[String]) -> R<()> {
    let ins = positional(args, &["-o", "--output"]);
    let input = PathBuf::from(ins.first().ok_or("render needs an input file")?);
    let doc = read_doc(&input)?;
    let out = output(args).unwrap_or_else(|| input.with_extension("png"));
    write_doc(&doc, &out)?;
    println!("{}", out.display());
    Ok(())
}

fn cmd_run(args: &[String]) -> R<()> {
    let ins = positional(args, &["-o", "--output", "--cmd"]);
    let input = PathBuf::from(ins.first().ok_or("run needs an input file")?);
    let mut session = Session::new(read_doc(&input)?);
    let mut i = 0;
    while let Some(pos) = args.iter().skip(i).position(|a| a == "--cmd") {
        let at = i + pos + 1;
        let spec = args.get(at).ok_or("--cmd needs ID[=JSON]")?;
        let (id, params) = match spec.split_once('=') {
            Some((id, p)) => (id, serde_json::from_str::<Value>(p).map_err(|e| format!("--cmd {id}: {e}"))?),
            None => (spec.as_str(), json!({})),
        };
        let out = session.execute(id, &params).map_err(|e| format!("{id}: {e}"))?;
        if !out.is_null() {
            println!("{out}");
        }
        i = at + 1;
    }
    if let Some(out) = output(args) {
        write_doc(&session.doc, &out)?;
        println!("{}", out.display());
    }
    Ok(())
}

fn cmd_stitch(args: &[String]) -> R<()> {
    let frames = positional(args, &["-o", "--output"]);
    if frames.len() < 2 {
        return Err("stitch needs two frames or more".to_string());
    }
    let mut s = screengrabcraft_render::stitch::Stitcher::new();
    for f in &frames {
        let bytes = std::fs::read(f).map_err(|e| format!("{f}: {e}"))?;
        let r = screengrabcraft_render::decode(&bytes).map_err(|e| format!("{f}: {e}"))?;
        eprintln!("{f}: {:?}", s.push(r));
    }
    let out = output(args).ok_or("stitch needs -o OUT")?;
    write_picture(&s.finish().ok_or("nothing to join")?, &out)?;
    println!("{}", out.display());
    Ok(())
}

fn cmd_service(what: Option<&str>) -> R<()> {
    use screengrabcraft_desktop::service;
    let msg = match what {
        Some("install") => {
            let exe = screengrabcraft_desktop::launch::sibling(screengrabcraft_desktop::launch::SERVICE_BIN)
                .ok_or("the capture service (screengrabcraft) is not installed next to this program")?;
            service::enable_with(&exe)?
        }
        Some("uninstall") => service::disable()?,
        Some("status") => {
            let s = service::status();
            format!("{} ({})", if s.enabled { "starts when you sign in" } else { "does not start when you sign in" }, s.how)
        }
        _ => return Err("usage: screengrabcraft-cli service install|uninstall|status".to_string()),
    };
    println!("{msg}");
    Ok(())
}
