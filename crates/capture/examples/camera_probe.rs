//! Tries a camera each way it offers its pictures, to see which way sends them (nothing is kept:
//! pictures are counted, then let go).
//!
//! `cargo run --release -p screengrabcraft-capture --example camera_probe -- list [camera]` lists
//! the cameras and the formats of one; `-- try <format number> [camera]` opens it in that format
//! for four seconds and says how many pictures came; `-- app [camera]` asks as the recorder does.
//! A camera stuck waiting for a picture never returns: run it with a time limit.

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn main() {
    use std::time::{Duration, Instant};

    use nokhwa::Camera;
    use nokhwa::pixel_format::RgbFormat;
    use nokhwa::utils::{ApiBackend, CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType, Resolution};

    let args: Vec<String> = std::env::args().skip(1).collect();
    // The camera in a program of its own, as the capture service runs it.
    if let Some(code) = screengrabcraft_capture::camera::helper_main(&args) {
        std::process::exit(i32::from(code));
    }
    if let Ok(exe) = std::env::current_exe() {
        screengrabcraft_capture::camera::use_helper(exe);
    }
    let what = args.first().map(String::as_str).unwrap_or("list");
    let camera = |at: usize| args.get(at).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    let open = |index: u32, wanted: RequestedFormatType| Camera::new(CameraIndex::Index(index), RequestedFormat::new::<RgbFormat>(wanted));
    let run = |mut cam: Camera| {
        if let Err(e) = cam.open_stream() {
            println!("could not start: {e}");
            return;
        }
        println!("started as {}", cam.camera_format());
        let t = Instant::now();
        let mut n = 0u32;
        while t.elapsed() < Duration::from_secs(4) {
            match cam.frame() {
                Ok(buf) => {
                    n += 1;
                    if n == 1 {
                        println!("first picture after {} ms: {:?} {}", t.elapsed().as_millis(), buf.source_frame_format(), buf.resolution());
                    }
                }
                Err(e) => {
                    println!("error after {} pictures: {e}", n);
                    break;
                }
            }
        }
        println!("pictures in 4 s: {n}");
        let _ = cam.stop_stream();
    };
    match what {
        "list" => {
            match nokhwa::query(ApiBackend::Auto) {
                Ok(list) => {
                    for c in list {
                        println!("camera {}: {}", c.index(), c.human_name());
                    }
                }
                Err(e) => println!("no list: {e}"),
            }
            match open(camera(1), RequestedFormatType::None) {
                Ok(mut cam) => match cam.compatible_camera_formats() {
                    Ok(fs) => {
                        for (i, f) in fs.iter().enumerate() {
                            println!("format {i}: {f}");
                        }
                    }
                    Err(e) => println!("no formats: {e}"),
                },
                Err(e) => println!("could not open: {e}"),
            }
        }
        "try" => {
            let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            let index = camera(2);
            let formats = open(index, RequestedFormatType::None).and_then(|mut c| c.compatible_camera_formats());
            let Some(f) = formats.ok().and_then(|fs| fs.get(n).copied()) else {
                println!("no format {n}");
                return;
            };
            println!("asking for {f}");
            match open(index, RequestedFormatType::Exact(f)) {
                Ok(cam) => run(cam),
                Err(e) => println!("could not open: {e}"),
            }
        }
        "thread" | "busy" | "mic" | "micwait" | "camfirst" => {
            // As a recording does it, on a thread of its own: (busy: the computer's sound first;
            // mic: the microphone too; camfirst: the camera first, the microphone once it sends
            // pictures), the cameras listed there, then the camera started. Sound is let go.
            let sound_on = what != "thread";
            let mic_first = what == "mic" || what == "micwait";
            let settle = if what == "micwait" { Duration::from_secs(3) } else { Duration::ZERO };
            let mic_after = what == "camfirst";
            let index = camera(1);
            let job = std::thread::spawn(move || {
                use screengrabcraft_capture::audio::{AudioCapture, AudioSource};
                let started = Instant::now();
                let mic = || {
                    let m = AudioCapture::start(AudioSource::Microphone(None), started);
                    println!("microphone: {}", m.as_ref().map(|c| c.name.clone()).unwrap_or_else(|e| e.to_string()));
                    m.ok()
                };
                let sound = if sound_on {
                    let s = AudioCapture::start(AudioSource::SystemSound(None), started);
                    println!("sound: {}", s.as_ref().map(|c| c.name.clone()).unwrap_or_else(|e| e.to_string()));
                    s.ok()
                } else {
                    None
                };
                let mut microphone = if mic_first { mic() } else { None };
                std::thread::sleep(settle);
                let listed = screengrabcraft_capture::camera::cameras();
                println!("listed: {listed:?}");
                match screengrabcraft_capture::camera::CameraCapture::start(index, started) {
                    Ok(cap) => {
                        println!("started {}", cap.name);
                        let t = Instant::now();
                        let (mut n, mut since_mic) = (0usize, 0usize);
                        let mut until = Duration::from_secs(4);
                        while t.elapsed() < until {
                            let got = cap.take().len();
                            if n == 0 && got > 0 {
                                println!("first picture after {} ms", t.elapsed().as_millis());
                                if mic_after {
                                    microphone = mic();
                                    until = t.elapsed() + Duration::from_secs(4);
                                }
                            }
                            n += got;
                            if microphone.is_some() {
                                since_mic += got;
                            }
                            if let Some(s) = &sound {
                                drop(s.take());
                            }
                            if let Some(m) = &microphone {
                                drop(m.take());
                            }
                            std::thread::sleep(Duration::from_millis(50));
                        }
                        println!("pictures: {n} ({since_mic} with the microphone on); trouble: {:?}", cap.trouble());
                        drop(cap.stop());
                    }
                    Err(e) => println!("could not start: {e}"),
                }
                if let Some(s) = sound {
                    drop(s.stop());
                }
                if let Some(m) = microphone {
                    drop(m.stop());
                }
            });
            let _ = job.join();
        }
        "micstop" => {
            // The camera's own microphone first, then the camera; after three seconds the
            // microphone is stopped: do pictures come then, does the camera stop, and can it be
            // opened again? (Heard and seen, then let go.)
            use screengrabcraft_capture::audio::{AudioCapture, AudioSource};
            use screengrabcraft_capture::camera::CameraCapture;
            let started = Instant::now();
            let mic = AudioCapture::start(AudioSource::Microphone(None), started);
            println!("microphone: {}", mic.as_ref().map(|c| c.name.clone()).unwrap_or_else(|e| e.to_string()));
            let Ok(cap) = CameraCapture::start(camera(1), started) else {
                println!("the camera didn't start");
                return;
            };
            let count = |cap: &CameraCapture, secs: u64| {
                let t = Instant::now();
                let mut n = 0usize;
                while t.elapsed() < Duration::from_secs(secs) {
                    n += cap.take().len();
                    std::thread::sleep(Duration::from_millis(50));
                }
                n
            };
            println!("pictures with the microphone on, 3 s: {}", count(&cap, 3));
            if let Ok(m) = mic {
                drop(m.stop());
            }
            println!("pictures after it stopped, 4 s: {}", count(&cap, 4));
            let t = Instant::now();
            drop(cap.stop());
            println!("the camera stopped in {} ms", t.elapsed().as_millis());
            std::thread::sleep(Duration::from_millis(500));
            match CameraCapture::start(camera(1), Instant::now()) {
                Ok(again) => {
                    println!("opened again: pictures in 3 s: {}; trouble: {:?}", count(&again, 3), again.trouble());
                    drop(again.stop());
                }
                Err(e) => println!("not opened again: {e}"),
            }
        }
        "recover" => {
            // As a recording now does it: the camera's own microphone on, the camera started; after
            // three seconds with no pictures the microphone is stopped and the camera stopped and
            // started again at once. (Heard and seen, then let go.)
            use screengrabcraft_capture::audio::{AudioCapture, AudioSource};
            use screengrabcraft_capture::camera::CameraCapture;
            let started = Instant::now();
            let mic = AudioCapture::start(AudioSource::Microphone(None), started);
            println!("microphone: {}", mic.as_ref().map(|c| c.name.clone()).unwrap_or_else(|e| e.to_string()));
            let Ok(cap) = CameraCapture::start(camera(1), started) else {
                println!("the camera didn't start");
                return;
            };
            std::thread::sleep(Duration::from_secs(3));
            println!("pictures with the microphone on, 3 s: {}", cap.take().len());
            if let Ok(m) = mic {
                drop(m.stop());
            }
            let t = Instant::now();
            cap.abandon();
            match CameraCapture::start(camera(1), started) {
                Ok(again) => {
                    println!("started again after {} ms", t.elapsed().as_millis());
                    let t = Instant::now();
                    let mut n = 0usize;
                    let mut first = None;
                    while t.elapsed() < Duration::from_secs(4) {
                        let got = again.take().len();
                        if got > 0 && first.is_none() {
                            first = Some(t.elapsed().as_millis());
                        }
                        n += got;
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    println!("pictures in the next 4 s: {n}, the first after {first:?} ms");
                    drop(again.stop());
                }
                Err(e) => println!("not started again: {e}"),
            }
        }
        "miconly" => {
            // The microphone alone for ten seconds (heard and let go), for a camera tried meanwhile
            // by another program.
            use screengrabcraft_capture::audio::{AudioCapture, AudioSource};
            match AudioCapture::start(AudioSource::Microphone(args.get(1).cloned()), Instant::now()) {
                Ok(m) => {
                    println!("microphone: {}", m.name);
                    let t = Instant::now();
                    let mut blocks = 0usize;
                    while t.elapsed() < Duration::from_secs(10) {
                        blocks += m.take().len();
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    println!("sound blocks in 10 s: {blocks}");
                    drop(m.stop());
                }
                Err(e) => println!("no microphone: {e}"),
            }
        }
        "service" => {
            // As the capture service does it: the cameras listed on this thread first (for the
            // Recorder), then recorded on a thread of their own, through ScreenGrabCraft's code.
            let listed = screengrabcraft_capture::camera::cameras();
            println!("listed: {listed:?}");
            match screengrabcraft_capture::camera::CameraCapture::start(camera(1), Instant::now()) {
                Ok(cap) => {
                    println!("started {}", cap.name);
                    let t = Instant::now();
                    let mut n = 0usize;
                    while t.elapsed() < Duration::from_secs(4) {
                        let got = cap.take().len();
                        if n == 0 && got > 0 {
                            println!("first picture after {} ms", t.elapsed().as_millis());
                        }
                        n += got;
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    println!("pictures in 4 s: {n}; trouble: {:?}", cap.trouble());
                    drop(cap.stop());
                }
                Err(e) => println!("could not start: {e}"),
            }
        }
        _ => {
            // As the recorder asks.
            let wanted = CameraFormat::new(Resolution::new(1280, 720), FrameFormat::MJPEG, 30);
            match open(camera(1), RequestedFormatType::Closest(wanted)) {
                Ok(cam) => run(cam),
                Err(e) => println!("could not open: {e}"),
            }
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn main() {
    println!("cameras are recorded on Windows and macOS only");
}
