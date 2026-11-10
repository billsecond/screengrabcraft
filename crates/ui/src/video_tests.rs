//! The video editor driven headless: opening a recording, splitting, cutting, undoing, the
//! camera's place.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use screengrabcraft_media::recording::{Header, RecordingWriter, StreamInfo, StreamKind};
use screengrabcraft_media::timeline::{CameraShape, Corner};
use screengrabcraft_model::{Color, EditorSettings, Raster};

use crate::editor::Editor;
use crate::services::{NoServices, OpenedFile};

/// A made-up recording: 4 s of a screen with a box moving, a microphone humming, a camera.
fn recording() -> Vec<u8> {
    let streams = vec![
        StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 320, height: 180, channels: 0 },
        StreamInfo { id: 2, kind: StreamKind::Microphone, name: "Desk microphone".into(), width: 0, height: 0, channels: 1 },
        StreamInfo { id: 3, kind: StreamKind::Camera, name: "Camera".into(), width: 64, height: 48, channels: 0 },
    ];
    let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
    let mut jpeg = Vec::new();
    let cam = image::RgbImage::from_pixel(64, 48, image::Rgb([30, 160, 140]));
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 80).encode_image(&cam).unwrap();
    for t in 0..40u64 {
        let mut pic = Raster::filled(320, 180, Color::WHITE).unwrap();
        for y in 40..90 {
            for x in (t * 6) as u32..(t * 6) as u32 + 50 {
                let i = ((y * 320 + x) * 4) as usize;
                pic.pixels_mut()[i..i + 3].copy_from_slice(&[0x4E, 0x3C, 0xA8]);
            }
        }
        w.video_frame(0, t * 100_000, &pic).unwrap();
        let hum: Vec<i16> = (0..4800).map(|i| (((t * 4800 + i) as f32 * 0.06).sin() * 9000.0) as i16).collect();
        w.audio(2, t * 4800, &hum).unwrap();
        w.jpeg_frame(3, t * 100_000, &jpeg).unwrap();
    }
    w.finish(4_000_000).unwrap()
}

fn harness() -> Harness<'static, Editor> {
    let mut ed = Editor::new(Box::new(NoServices), EditorSettings::default());
    ed.open_file(&egui::Context::default(), OpenedFile { name: "Production.sgvideo".into(), path: None, bytes: recording() });
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).build_ui_state(
        |ui, ed: &mut Editor| {
            let ctx = ui.ctx().clone();
            ed.logic(&ctx);
            ed.ui(ui);
        },
        ed,
    );
    h.run_steps(3);
    h
}

fn length(h: &Harness<'_, Editor>) -> u64 {
    h.state().video.as_ref().map(|v| v.session.timeline.length()).unwrap_or(0)
}

#[test]
fn a_recording_opens_in_the_video_editor_with_a_row_for_each_track() {
    let mut h = harness();
    let v = h.state().video.as_ref().expect("the video editor is open");
    assert_eq!(v.session.timeline.tracks.len(), 3);
    assert_eq!(length(&h), 4_000_000);
    for label in ["Export Video", "Split", "Show", "Top left", "Circle"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    if let Ok(dir) = std::env::var("SGC_SHOTS")
        && let Ok(img) = h.render()
    {
        let _ = img.save(format!("{dir}/video-editor.png"));
    }
}

#[test]
fn split_then_cut_a_stretch_then_undo() {
    let mut h = harness();
    if let Some(v) = h.state_mut().video.as_mut() {
        v.playhead = 1_500_000;
    }
    h.get_by_label("Split").click();
    h.run_steps(2);
    let clips: Vec<usize> = h.state().video.as_ref().unwrap().session.timeline.tracks.iter().map(|t| t.clips.len()).collect();
    assert_eq!(clips, vec![2, 2, 2], "every track split at the playhead");
    if let Some(v) = h.state_mut().video.as_mut() {
        v.range = Some((1_000_000, 2_000_000));
    }
    h.run_steps(1);
    h.get_by_label("Cut").click();
    h.run_steps(2);
    assert_eq!(length(&h), 3_000_000);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(2);
    assert_eq!(length(&h), 4_000_000);
}

#[test]
fn space_plays_and_the_playhead_moves() {
    let mut h = harness();
    h.key_press(egui::Key::Space);
    h.run_steps(1);
    assert!(h.state().video.as_ref().unwrap().is_playing());
    std::thread::sleep(std::time::Duration::from_millis(120));
    h.run_steps(2);
    assert!(h.state().video.as_ref().unwrap().playhead > 0);
    h.key_press(egui::Key::Space);
    h.run_steps(1);
    assert!(!h.state().video.as_ref().unwrap().is_playing());
}

#[test]
fn the_camera_moves_to_another_corner_and_becomes_a_circle() {
    let mut h = harness();
    h.get_by_label("Top left").click();
    h.run_steps(2);
    h.get_by_label("Circle").click();
    h.run_steps(2);
    let cam = h.state().video.as_ref().unwrap().session.timeline.camera;
    assert_eq!((cam.corner, cam.shape), (Corner::TopLeft, CameraShape::Circle));
}

#[test]
fn deleting_a_selected_clip_leaves_a_gap_on_its_track_only() {
    let mut h = harness();
    if let Some(v) = h.state_mut().video.as_mut() {
        v.selected = Some((1, 0));
    }
    h.run_steps(1);
    h.key_press(egui::Key::Delete);
    h.run_steps(2);
    let v = h.state().video.as_ref().unwrap();
    assert!(v.session.timeline.tracks[1].clips.is_empty());
    assert_eq!(v.session.timeline.tracks[0].clips.len(), 1);
    assert_eq!(length(&h), 4_000_000);
}

#[test]
fn a_damaged_recording_is_an_error_not_a_crash() {
    let mut bytes = recording();
    bytes.truncate(bytes.len() / 3);
    let mut ed = Editor::new(Box::new(NoServices), EditorSettings::default());
    ed.open_file(&egui::Context::default(), OpenedFile { name: "Cut short.sgvideo".into(), path: None, bytes });
    // What was written opens.
    assert!(ed.video.is_some());
    let mut junk = b"SGVIDEO\x01".to_vec();
    junk.extend_from_slice(&[0xFF; 40]);
    let mut ed = Editor::new(Box::new(NoServices), EditorSettings::default());
    ed.open_file(&egui::Context::default(), OpenedFile { name: "Junk.sgvideo".into(), path: None, bytes: junk });
    assert!(ed.video.is_none());
}

#[test]
fn show_unticked_hides_a_track_and_ticked_shows_it_again() {
    let mut h = harness();
    // The tracks: screen, microphone, camera; each row has its Show switch.
    let shows: Vec<_> = h.query_all_by_label("Show").collect();
    assert_eq!(shows.len(), 3);
    let cam = 2;
    h.query_all_by_label("Show").nth(cam).unwrap().click();
    h.run_steps(2);
    assert!(h.state().video.as_ref().unwrap().session.timeline.tracks[cam].muted, "hidden");
    h.query_all_by_label("Show").nth(cam).unwrap().click();
    h.run_steps(2);
    assert!(!h.state().video.as_ref().unwrap().session.timeline.tracks[cam].muted, "shown again");
    // The microphone's: muted.
    h.query_all_by_label("Show").nth(1).unwrap().click();
    h.run_steps(2);
    assert!(h.state().video.as_ref().unwrap().session.timeline.tracks[1].muted);
}

/// A real recording (`SGC_RECORDING`, as `live_camera_and_microphone` keeps it with `SGC_KEEP`)
/// opened in the editor as the owner opens one: its camera must be in the picture shown at the
/// start, and gone once its Show is unticked. Only a count of pixels is printed; nothing is
/// saved. Run by hand only: `cargo test -p screengrabcraft-ui live_recording -- --ignored`.
#[test]
#[ignore]
fn live_recording_shows_its_camera_in_the_editor() {
    let path = std::env::var("SGC_RECORDING").expect("SGC_RECORDING names a recording");
    let bytes = std::fs::read(&path).unwrap();
    let mut ed = Editor::new(Box::new(NoServices), EditorSettings::default());
    ed.open_file(&egui::Context::default(), OpenedFile { name: "Live.sgvideo".into(), path: None, bytes });
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).build_ui_state(
        |ui, ed: &mut Editor| {
            let ctx = ui.ctx().clone();
            ed.logic(&ctx);
            ed.ui(ui);
        },
        ed,
    );
    h.run_steps(4);
    let v = h.state().video.as_ref().expect("it opens in the video editor");
    let cam = v.session.timeline.find(StreamKind::Camera).expect("a camera track");
    let rows = v.session.timeline.tracks.len();
    let mut differ_at = Vec::new();
    for at in [100_000u64, 1_000_000] {
        if let Some(v) = h.state_mut().video.as_mut() {
            v.playhead = at;
        }
        h.run_steps(3);
        let shown = h.render().unwrap();
        h.query_all_by_label("Show").nth(cam).unwrap().click();
        h.run_steps(3);
        assert!(h.state().video.as_ref().unwrap().session.timeline.tracks[cam].muted, "hidden");
        let hidden = h.render().unwrap();
        h.query_all_by_label("Show").nth(cam).unwrap().click();
        h.run_steps(3);
        let differ = shown.pixels().zip(hidden.pixels()).filter(|(a, b)| a != b).count();
        println!("at {:.1} s: {differ} pixels of the editor differ with the camera hidden ({rows} tracks)", at as f64 / 1e6);
        differ_at.push(differ);
    }
    assert!(differ_at.iter().all(|d| *d > 2_000), "the camera is seen in the editor: {differ_at:?}");
}
