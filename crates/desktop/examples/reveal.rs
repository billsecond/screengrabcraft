//! Shows a file selected in the file manager, as Show in Folder does:
//! `cargo run -p screengrabcraft-desktop --example reveal -- <file>`.

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        println!("say which file");
        return;
    };
    match screengrabcraft_desktop::launch::reveal(std::path::Path::new(&path)) {
        Ok(()) => println!("asked the file manager to show {path}"),
        Err(e) => println!("couldn't: {e}"),
    }
    // It's asked on a thread of its own: this program waits for it rather than ending under it.
    std::thread::sleep(std::time::Duration::from_secs(2));
}
