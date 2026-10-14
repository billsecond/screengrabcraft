//! Windows only: embed the icon, the version info and an application manifest into
//! `screengrabcraft-cli.exe`. The manifest makes the program per-monitor DPI aware: without it
//! Windows hands a console program scaled coordinates and the captures come out at the wrong
//! size on a scaled display.

const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>
"#;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/screengrabcraft.ico");
    println!("cargo:rerun-if-env-changed=SCREENGRABCRAFT_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/screengrabcraft.ico")
        .set_manifest(MANIFEST)
        .set("ProductName", "ScreenGrabCraft")
        .set("FileDescription", "ScreenGrabCraft command line")
        .set("CompanyName", "ScreenGrabCraft contributors")
        .set("LegalCopyright", "Copyright (c) the ScreenGrabCraft authors. MIT OR Apache-2.0.")
        .set("OriginalFilename", "screengrabcraft-cli.exe")
        .set("InternalName", "screengrabcraft-cli");
    if let Err(e) = res.compile() {
        if std::env::var_os("SCREENGRABCRAFT_REQUIRE_WINRES").is_some() {
            eprintln!("embedding Windows resources failed: {e}");
            std::process::exit(1);
        }
        println!("cargo:warning=screengrabcraft-cli.exe built without icon/version resources: {e}");
    }
}
