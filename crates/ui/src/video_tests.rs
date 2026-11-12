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
    for label in ["Export Video", "Split", "Mute", "Top left", "Circle"] {
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

/// A host whose capture service makes captions when asked, and says they've come when told.
#[derive(Clone, Default)]
struct CaptionHost {
    asked: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    ready: std::sync::Arc<std::sync::Mutex<Option<screengrabcraft_media::captions::Captions>>>,
    jobs: std::sync::Arc<std::sync::Mutex<Vec<(Option<usize>, bool)>>>,
}

impl crate::services::Services for CaptionHost {
    fn append_file(&self, _path: &str, _bytes: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn poll_captions(&self, _recording: &str, seen: u64) -> Option<(u64, Option<screengrabcraft_media::captions::Captions>)> {
        let ready = self.ready.lock().unwrap().clone();
        (ready.is_some() && seen != 7).then_some((7, ready))
    }
    fn make_captions(&self, recording: &str) -> Result<(), String> {
        self.asked.lock().unwrap().push(recording.to_string());
        Ok(())
    }
    fn export_video(&self, job: crate::services::VideoJob) -> Result<crate::services::VideoExport, String> {
        self.jobs.lock().unwrap().push((job.captions.as_ref().map(Vec::len), job.burn));
        Ok(crate::services::VideoExport::default())
    }
}

/// A made-up meeting: 4 s of a screen, the computer's sound and a microphone, each humming.
fn meeting() -> Vec<u8> {
    let streams = vec![
        StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 320, height: 180, channels: 0 },
        StreamInfo { id: 1, kind: StreamKind::SystemSound, name: "Speakers".into(), width: 0, height: 0, channels: 2 },
        StreamInfo { id: 2, kind: StreamKind::Microphone, name: "Desk microphone".into(), width: 0, height: 0, channels: 1 },
    ];
    let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
    for t in 0..40u64 {
        let shade = 200 - (t * 3) as u8;
        w.video_frame(0, t * 100_000, &Raster::filled(320, 180, Color::rgb(shade, shade, 230)).unwrap()).unwrap();
        let hum: Vec<i16> = (0..4800).map(|i| (((t * 4800 + i) as f32 * 0.06).sin() * 9000.0) as i16).collect();
        let stereo: Vec<i16> = hum.iter().flat_map(|v| [*v / 2, *v / 2]).collect();
        w.audio(1, t * 4800, &stereo).unwrap();
        w.audio(2, t * 4800, &hum).unwrap();
    }
    w.finish(4_000_000).unwrap()
}

fn captions() -> screengrabcraft_media::captions::Captions {
    use screengrabcraft_media::captions::{Captions, Cue, Speaker};
    Captions {
        cues: vec![
            Cue { start: 200_000, end: 1_800_000, speaker: Speaker::You, text: "Here's the new login page.".into() },
            Cue { start: 2_000_000, end: 3_600_000, speaker: Speaker::Others, text: "It looks good to me.".into() },
        ],
        model: "test".into(),
    }
}

fn caption_harness(bytes: Vec<u8>, host: CaptionHost) -> Harness<'static, Editor> {
    let mut ed = Editor::new(Box::new(host), EditorSettings::default());
    let path = Some("C:/Captures/Production.sgvideo".to_string());
    ed.open_file(&egui::Context::default(), OpenedFile { name: "Production.sgvideo".into(), path, bytes });
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 860.0)).build_ui_state(
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

#[test]
fn captions_show_on_their_row_and_on_the_picture_and_go_into_the_video() {
    let mut bytes = meeting();
    bytes.extend(screengrabcraft_media::recording::captions_bytes(&captions()).unwrap());
    let host = CaptionHost::default();
    let mut h = caption_harness(bytes, host.clone());
    {
        let v = h.state().video.as_ref().unwrap();
        assert_eq!(v.cues.len(), 2);
    }
    for label in ["Captions", "Show", "In video"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    // In video, then export: the job carries the captions and burns them.
    h.get_by_label("In video").click();
    h.run_steps(2);
    assert!(h.state().video.as_ref().unwrap().session.timeline.captions.burn);
    if let Some(v) = h.state_mut().video.as_mut() {
        v.playhead = 2_500_000;
    }
    h.run_steps(2);
    if let Ok(dir) = std::env::var("SGC_SHOTS")
        && let Ok(img) = h.render()
    {
        let _ = img.save(format!("{dir}/video-captions.png"));
    }
    h.get_by_label("Export Video").click();
    h.run_steps(2);
    assert_eq!(host.jobs.lock().unwrap().as_slice(), &[(Some(2), true)]);
    // Muting the microphone takes your line off the video (and the timeline).
    if let Some(v) = h.state_mut().video.as_mut() {
        let mic = v.session.timeline.find(StreamKind::Microphone).unwrap();
        v.session.execute("video.mute", &serde_json::json!({"track": mic, "muted": true})).unwrap();
    }
    h.run_steps(2);
    assert_eq!(h.state().video.as_ref().unwrap().cues.len(), 1);
    // Show off: still on the timeline, no longer on the picture.
    h.get_by_label("Show").click();
    h.run_steps(2);
    assert!(!h.state().video.as_ref().unwrap().session.timeline.captions.show);
}

#[test]
fn captions_can_be_asked_for_and_appear_when_theyre_made() {
    let host = CaptionHost::default();
    let mut h = caption_harness(meeting(), host.clone());
    assert!(h.state().video.as_ref().unwrap().captions.is_none());
    h.get_by_label("Make Captions").click();
    h.run_steps(2);
    assert_eq!(host.asked.lock().unwrap().as_slice(), ["C:/Captures/Production.sgvideo"]);
    assert!(h.state().video.as_ref().unwrap().captions_coming.is_some());
    assert!(h.query_all_by_label("Make Captions").next().is_none(), "asked once");
    // The service has made them: they come on the next look.
    *host.ready.lock().unwrap() = Some(captions());
    if let Some(v) = h.state_mut().video.as_mut() {
        v.expect_captions(0.0);
    }
    h.run_steps(3);
    let v = h.state().video.as_ref().unwrap();
    assert_eq!(v.captions.as_ref().map(|c| c.cues.len()), Some(2));
    assert!(v.captions_coming.is_none());
    assert_eq!(v.cues.len(), 2);
}

#[test]
fn a_recording_with_no_words_stops_waiting_for_captions() {
    let host = CaptionHost::default();
    let mut h = caption_harness(meeting(), host.clone());
    h.get_by_label("Make Captions").click();
    h.run_steps(2);
    assert!(h.state().video.as_ref().unwrap().captions_coming.is_some());
    // The service heard nothing: empty captions, and the editor stops saying they're coming.
    *host.ready.lock().unwrap() = Some(screengrabcraft_media::captions::Captions::default());
    if let Some(v) = h.state_mut().video.as_mut() {
        v.expect_captions(0.0);
    }
    h.run_steps(3);
    let v = h.state().video.as_ref().unwrap();
    assert!(v.captions.is_none() && v.captions_coming.is_none());
}
