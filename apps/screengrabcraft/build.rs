//! Windows only: embed the capture service's icon and version info into `screengrabcraft.exe`, so
//! it shows in Explorer, the taskbar, the Start menu and Alt-Tab. A missing resource compiler is
//! a warning unless `SCREENGRABCRAFT_REQUIRE_WINRES=1` (release builds) makes it an error.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/screengrabcraft.ico");
    println!("cargo:rerun-if-env-changed=SCREENGRABCRAFT_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/screengrabcraft.ico")
        .set("ProductName", "ScreenGrabCraft")
        .set("FileDescription", "ScreenGrabCraft screen capture")
        .set("CompanyName", "ScreenGrabCraft contributors")
        .set("LegalCopyright", "Copyright (c) the ScreenGrabCraft authors. MIT OR Apache-2.0.")
        .set("OriginalFilename", "screengrabcraft.exe")
        .set("InternalName", "screengrabcraft");
    if let Err(e) = res.compile() {
        if std::env::var_os("SCREENGRABCRAFT_REQUIRE_WINRES").is_some() {
            eprintln!("embedding Windows resources failed: {e}");
            std::process::exit(1);
        }
        println!("cargo:warning=screengrabcraft.exe built without icon/version resources: {e}");
    }
}
