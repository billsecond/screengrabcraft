//! The update check, live: asks GitHub for the latest release the way the capture service does,
//! and can download and check the Windows installer. Nothing is installed.
//!
//! ```text
//! cargo run -p screengrabcraft-desktop --example update_check
//! cargo run -p screengrabcraft-desktop --example update_check -- --as 0.1.6
//! cargo run -p screengrabcraft-desktop --example update_check -- --as 0.1.6 --download <folder> [--arch x86_64|aarch64|x86]
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use screengrabcraft_desktop::update::{self, Found};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let Some(current) = value("--as").map_or_else(update::current, |v| update::parse_version(&v)) else {
        eprintln!("not a version");
        return ExitCode::FAILURE;
    };
    println!("asking {} as ScreenGrabCraft {current} ({})", update::LATEST, update::user_agent());
    let release = match update::check(&current) {
        Ok(Found::UpToDate(latest)) => {
            match latest {
                Some(v) => println!("up to date: the latest release is {v}"),
                None => println!("up to date: no release to offer"),
            }
            None
        }
        Ok(Found::Newer(r)) => {
            println!("update available: {} (what's new: {})", r.version, r.page());
            Some(r)
        }
        Err(e) => {
            eprintln!("the check failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let Some(dir) = value("--download").map(PathBuf::from) else { return ExitCode::SUCCESS };
    let Some(release) = release else {
        println!("nothing to download");
        return ExitCode::SUCCESS;
    };
    let arch = value("--arch").unwrap_or_else(|| std::env::consts::ARCH.to_string());
    match update::pick_installer(&release, "windows", &arch) {
        Some(a) => println!("installer for windows {arch}: {} ({} bytes)", a.name, a.size),
        None => {
            eprintln!("no installer for windows {arch}");
            return ExitCode::FAILURE;
        }
    }
    match update::download(&release, &dir, "windows", &arch) {
        Ok(i) => {
            let size = std::fs::metadata(&i.path).map(|m| m.len()).unwrap_or(0);
            println!("downloaded and checked against {}: {} ({size} bytes)", update::SUMS, i.path.display());
            println!("SHA-256 {}", update::hex(&i.sha256));
            match update::verify_or_delete(&i.path, &i.sha256) {
                Ok(()) => println!("checked again: it matches"),
                Err(e) => println!("checked again: {e}"),
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("the download failed: {e}");
            ExitCode::FAILURE
        }
    }
}
