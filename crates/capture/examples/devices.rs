//! Lists the sound devices and cameras a video recording can use, by name only: nothing is
//! turned on. `cargo run -p screengrabcraft-capture --example devices`
fn main() {
    println!("microphones: {:?}", screengrabcraft_capture::audio::microphones());
    println!("computer sound can be recorded: {}", screengrabcraft_capture::audio::system_sound_supported());
    println!("cameras: {:?}", screengrabcraft_capture::camera::cameras());
}
