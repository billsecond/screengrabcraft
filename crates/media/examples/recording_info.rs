//! What a recording (`.sgvideo`) holds, by numbers only: its tracks, and for each how many
//! pictures and when the first and last are, or how long its sound runs. Nothing of the pictures
//! or the sound is shown. `cargo run -p screengrabcraft-media --example recording_info -- <file>`
fn main() {
    let Some(path) = std::env::args().nth(1) else {
        println!("usage: recording_info <file.sgvideo>");
        return;
    };
    let source = match screengrabcraft_media::recording::FileSource::open(std::path::Path::new(&path)) {
        Ok(s) => s,
        Err(e) => {
            println!("can't open it: {e}");
            return;
        }
    };
    let rec = match screengrabcraft_media::recording::Recording::open(Box::new(source)) {
        Ok(r) => r,
        Err(e) => {
            println!("can't read it: {e}");
            return;
        }
    };
    let s = |us: u64| format!("{:.2} s", us as f64 / 1e6);
    println!("finished: {}, length {}, edits: {}", rec.finished, s(rec.duration_us), rec.timeline.is_some());
    for st in &rec.header.streams {
        let frames = rec.frames(st.id);
        if st.kind.is_video() {
            let first = frames.first().map(|f| s(f.pts)).unwrap_or_else(|| "-".to_string());
            let last = frames.last().map(|f| s(f.pts)).unwrap_or_else(|| "-".to_string());
            println!("track {} {:?} \"{}\" {}x{}: {} pictures, from {first} to {last}", st.id, st.kind, st.name, st.width, st.height, frames.len());
        } else {
            println!("track {} {:?} \"{}\": sound to {}", st.id, st.kind, st.name, s(rec.stream_duration(st.id)));
        }
    }
}
