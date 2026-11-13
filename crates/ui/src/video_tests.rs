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

/// A host whose capture service makes captions when asked, and says they've come when told.
#[derive(Clone, Default)]
struct CaptionHost {
    asked: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    ready: std::sync::Arc<std::sync::Mutex<Option<screengrabcraft_media::captions::Captions>>>,
    jobs: std::sync::Arc<std::sync::Mutex<Vec<Job>>>,
    /// The last video being made, to finish by hand.
    making: std::sync::Arc<std::sync::Mutex<Option<crate::services::VideoExport>>>,
    /// The names offered in save dialogs, the answer the next dialog gives, the files written.
    offered: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    answer: std::sync::Arc<std::sync::Mutex<Option<crate::services::DialogResult>>>,
    written: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
    /// Files shown in their folder (`folder:`) or opened (`open:`).
    shown: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

/// What a video job asked for.
#[derive(Clone, Debug, PartialEq)]
struct Job {
    out: String,
    replace: bool,
    format: crate::services::VideoFormat,
    captions: Option<usize>,
    burn: bool,
    subtitles: bool,
}

impl crate::services::Services for CaptionHost {
    fn video_formats(&self) -> Vec<crate::services::VideoFormat> {
        use crate::services::VideoFormat;
        vec![VideoFormat::Mp4, VideoFormat::Mp4Av1, VideoFormat::Gif]
    }
    fn open_folder(&self, path: Option<&str>) {
        self.shown.lock().unwrap().push(format!("folder:{}", path.unwrap_or("")));
    }
    fn open_with_system(&self, path: &str) {
        self.shown.lock().unwrap().push(format!("open:{path}"));
    }
    fn request_save(&self, suggested_name: &str, _filters: &[crate::services::Filter]) {
        self.offered.lock().unwrap().push(suggested_name.to_string());
    }
    fn poll_dialog(&self) -> Option<crate::services::DialogResult> {
        self.answer.lock().unwrap().take()
    }
    fn write_file(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        self.written.lock().unwrap().push((path.to_string(), String::from_utf8_lossy(bytes).to_string()));
        Ok(())
    }
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
        self.jobs.lock().unwrap().push(Job {
            out: job.out.clone(),
            replace: job.replace,
            format: job.format,
            captions: job.captions.as_ref().map(Vec::len),
            burn: job.burn,
            subtitles: job.subtitles,
        });
        let x = crate::services::VideoExport::default();
        *self.making.lock().unwrap() = Some(x.clone());
        Ok(x)
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
    for label in ["Captions", "Show"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    if let Some(v) = h.state_mut().video.as_mut() {
        v.playhead = 2_500_000;
    }
    h.run_steps(2);
    if let Ok(dir) = std::env::var("SGC_SHOTS")
        && let Ok(img) = h.render()
    {
        let _ = img.save(format!("{dir}/video-captions.png"));
    }
    // Export Video, captions on the picture: the job carries the captions and burns them.
    h.get_by_label("Export Video").click();
    h.run_steps(2);
    h.get_by_label("On the picture (always shown)").click();
    h.run_steps(1);
    h.get_by_label("Export").click();
    h.run_steps(2);
    let job = host.jobs.lock().unwrap().pop().unwrap();
    assert_eq!((job.captions, job.burn, job.subtitles), (Some(2), true, false));
    // Done: the window says so (the next test looks at it); closed here.
    *host.making.lock().unwrap().as_ref().unwrap().done.lock().unwrap() = Some(Ok("C:/Captures/Production.mp4".into()));
    h.run_steps(2);
    h.get_by_label("Close").click();
    h.run_steps(2);
    assert!(h.state().video.as_ref().unwrap().export.is_none());
    // Muting the microphone takes your line off the video (and the timeline).
    if let Some(v) = h.state_mut().video.as_mut() {
        let mic = v.session.timeline.find(StreamKind::Microphone).unwrap();
        v.session.execute("video.mute", &serde_json::json!({"track": mic, "muted": true})).unwrap();
    }
    h.run_steps(2);
    assert_eq!(h.state().video.as_ref().unwrap().cues.len(), 1);
    // The captions' Show (the last row's) off: still on the timeline, no longer on the picture.
    h.query_all_by_label("Show").last().unwrap().click();
    h.run_steps(2);
    assert!(!h.state().video.as_ref().unwrap().session.timeline.captions.show);
}

#[test]
fn export_transcript_saves_what_was_said_as_text_or_subtitles() {
    use crate::services::DialogResult;
    let mut bytes = meeting();
    bytes.extend(screengrabcraft_media::recording::captions_bytes(&captions()).unwrap());
    let host = CaptionHost::default();
    let mut h = caption_harness(bytes, host.clone());
    let save = |h: &mut Harness<'_, Editor>, to: &str| {
        h.get_by_label("Export Transcript").click();
        h.run_steps(2);
        *host.answer.lock().unwrap() = Some(DialogResult::SaveTo(to.to_string()));
        h.run_steps(2);
        host.written.lock().unwrap().pop().unwrap()
    };
    // A transcript: both speakers, with when each started. Then its folder is shown, with it
    // selected.
    let (path, text) = save(&mut h, "C:/Notes/Meeting.txt");
    assert_eq!(host.offered.lock().unwrap().as_slice(), ["Production transcript.txt"]);
    assert_eq!(path, "C:/Notes/Meeting.txt");
    assert_eq!(host.shown.lock().unwrap().as_slice(), ["folder:C:/Notes/Meeting.txt"]);
    assert_eq!(text, "[0:00:00] You: Here's the new login page.\n[0:00:02] Others: It looks good to me.\n");
    // A name ending .srt is subtitles.
    let (_, srt) = save(&mut h, "C:/Notes/Meeting.srt");
    assert!(srt.starts_with("1\n00:00:00,200 --> 00:00:01,800\nYou: Here's the new login page."), "{srt}");
    // A name with no extension gets the transcript's.
    let (path, _) = save(&mut h, "C:/Notes/Meeting");
    assert_eq!(path, "C:/Notes/Meeting.txt");
    // As edited: the microphone muted, your line is left out.
    if let Some(v) = h.state_mut().video.as_mut() {
        let mic = v.session.timeline.find(StreamKind::Microphone).unwrap();
        v.session.execute("video.mute", &serde_json::json!({"track": mic, "muted": true})).unwrap();
    }
    h.run_steps(2);
    let (_, text) = save(&mut h, "C:/Notes/Others.txt");
    assert_eq!(text, "[0:00:02] Others: It looks good to me.\n");
    // Cancelled, then a later save dialog's answer isn't taken for a transcript.
    h.get_by_label("Export Transcript").click();
    h.run_steps(2);
    *host.answer.lock().unwrap() = Some(DialogResult::Cancelled);
    h.run_steps(2);
    assert!(h.state().saving_transcript.is_none());
    assert!(host.written.lock().unwrap().is_empty());
}

#[test]
fn export_transcript_waits_for_captions() {
    let host = CaptionHost::default();
    let mut h = caption_harness(meeting(), host.clone());
    // A recording with no captions yet: they're asked for first, and nothing is saved meanwhile.
    h.get_by_label("Export Transcript").click();
    h.run_steps(2);
    assert_eq!(host.asked.lock().unwrap().as_slice(), ["C:/Captures/Production.sgvideo"]);
    assert!(host.offered.lock().unwrap().is_empty());
    assert!(h.state().video.as_ref().unwrap().transcript_after.is_some());
    // They've come: where the transcript goes is asked at once.
    *host.ready.lock().unwrap() = Some(captions());
    if let Some(v) = h.state_mut().video.as_mut() {
        v.expect_captions(0.0);
    }
    h.run_steps(3);
    assert_eq!(host.offered.lock().unwrap().as_slice(), ["Production transcript.txt"]);
    assert!(h.state().video.as_ref().unwrap().transcript_after.is_none());
}

#[test]
fn export_video_asks_where_and_how_shows_its_progress_then_play_and_show_in_folder() {
    use crate::services::{DialogResult, VideoFormat};
    let mut bytes = meeting();
    bytes.extend(screengrabcraft_media::recording::captions_bytes(&captions()).unwrap());
    let host = CaptionHost::default();
    let mut h = caption_harness(bytes, host.clone());
    let shot = |h: &mut Harness<'_, Editor>, name: &str| {
        if let Ok(dir) = std::env::var("SGC_SHOTS")
            && let Ok(img) = h.render()
        {
            let _ = img.save(format!("{dir}/{name}.png"));
        }
    };
    h.get_by_label("Export Video").click();
    h.run_steps(2);
    // Beside the recording, every player's MP4, the captions as subtitles beside it.
    for label in ["Save as", "Format", "Closed captions (AI generated)", "Production.mp4", "As a subtitles file beside it (.srt)"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    shot(&mut h, "export-video");
    // Change…: the save dialog, and a name without its extension gets it.
    h.get_by_label("Change…").click();
    h.run_steps(2);
    assert_eq!(host.offered.lock().unwrap().as_slice(), ["Production.mp4"]);
    *host.answer.lock().unwrap() = Some(DialogResult::SaveTo("D:/Videos/Demo".into()));
    h.run_steps(2);
    assert!(h.query_all_by_label("Demo.mp4").next().is_some());
    h.get_by_label("Export").click();
    h.run_steps(2);
    let job = host.jobs.lock().unwrap().pop().unwrap();
    assert_eq!(
        job,
        Job { out: "D:/Videos/Demo.mp4".into(), replace: true, format: VideoFormat::Mp4, captions: Some(2), burn: false, subtitles: true }
    );
    // Being made: a window with its progress and Cancel; the tool bar has no Show in folder.
    let making = host.making.lock().unwrap().clone().unwrap();
    making.progress.store(400, std::sync::atomic::Ordering::SeqCst);
    h.run_steps(2);
    assert!(h.query_all_by_label("Exporting Video").next().is_some() || h.query_all_by_label_contains("Making").next().is_some());
    assert!(h.query_all_by_label("Show in folder").next().is_none());
    shot(&mut h, "export-video-making");
    // Made: Play Video, Show in Folder (the file selected), Close.
    *making.done.lock().unwrap() = Some(Ok("D:/Videos/Demo.mp4".into()));
    h.run_steps(2);
    for label in ["Play Video", "Show in Folder", "Close", "Its subtitles are beside it: Demo.srt"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    shot(&mut h, "export-video-made");
    h.get_by_label("Show in Folder").click();
    h.run_steps(2);
    assert_eq!(host.shown.lock().unwrap().as_slice(), ["folder:D:/Videos/Demo.mp4"]);
    assert!(h.state().video.as_ref().unwrap().export.is_none());
    // Offered again as it was last set.
    h.get_by_label("Export Video").click();
    h.run_steps(2);
    match &h.state().video.as_ref().unwrap().export {
        Some(crate::video::ExportStep::Choose(c)) => assert_eq!((c.format, c.captions), (VideoFormat::Mp4, crate::video::CaptionsIn::Subtitles)),
        _ => panic!("the Export Video window is open"),
    }
}

#[test]
fn a_gif_has_no_subtitles_and_cancelling_says_so() {
    use crate::services::VideoFormat;
    use crate::video::{CaptionsIn, ExportStep};
    let mut bytes = meeting();
    bytes.extend(screengrabcraft_media::recording::captions_bytes(&captions()).unwrap());
    let host = CaptionHost::default();
    let mut h = caption_harness(bytes, host.clone());
    h.get_by_label("Export Video").click();
    h.run_steps(2);
    // Picked as a name ending .gif in the save dialog: a GIF, its captions on the picture.
    if let Some(v) = h.state_mut().video.as_mut() {
        v.export_saved_to("C:/Clips/Demo.gif");
    }
    h.run_steps(2);
    match &h.state().video.as_ref().unwrap().export {
        Some(ExportStep::Choose(c)) => {
            assert_eq!((c.format, c.captions, c.path.as_str()), (VideoFormat::Gif, CaptionsIn::Picture, "C:/Clips/Demo.gif"));
        }
        _ => panic!("the Export Video window is open"),
    }
    h.get_by_label("Export").click();
    h.run_steps(2);
    let job = host.jobs.lock().unwrap().pop().unwrap();
    assert_eq!((job.format, job.burn, job.subtitles), (VideoFormat::Gif, true, false));
    // Cancel (once the window has settled where it's shown): the job is told, and when it
    // stops nothing is left open.
    h.run_steps(2);
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    let making = host.making.lock().unwrap().clone().unwrap();
    assert!(making.cancel.load(std::sync::atomic::Ordering::SeqCst));
    *making.done.lock().unwrap() = Some(Err("cancelled".into()));
    h.run_steps(2);
    assert!(h.state().video.as_ref().unwrap().export.is_none());
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

/// A host whose recordings are on its disk (here, in memory): they're read on a thread of their
/// own, as the desktop's are.
struct DiskHost {
    recording: Vec<u8>,
}

impl crate::services::Services for DiskHost {
    fn open_recording(&self, _path: &str) -> Result<Box<dyn screengrabcraft_media::recording::Source>, String> {
        Ok(Box::new(self.recording.clone()))
    }
    fn append_file(&self, _path: &str, _bytes: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn a_recording_from_the_disk_says_its_opening_then_opens() {
    let mut ed = Editor::new(Box::new(DiskHost { recording: meeting() }), EditorSettings::default());
    let ctx = egui::Context::default();
    ed.open_path(&ctx, "C:/Captures/Long meeting.sgvideo");
    ed.expect_captions(0.0);
    assert!(ed.opening_video.is_some(), "read on a thread of its own");
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).build_ui_state(
        |ui, ed: &mut Editor| {
            let ctx = ui.ctx().clone();
            ed.logic(&ctx);
            ed.ui(ui);
        },
        ed,
    );
    h.run_steps(1);
    // Read meanwhile: the window says so (unless it was quicker than a frame).
    let shown = h.query_by_label("Opening the Recording").is_some();
    for _ in 0..200 {
        if h.state().video.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.run_steps(1);
    }
    let v = h.state().video.as_ref().expect("the recording opened");
    assert_eq!(v.session.timeline.length(), 4_000_000);
    assert!(v.captions_coming.is_some(), "captions said to be coming while it was read are waited for");
    assert!(h.state().opening_video.is_none());
    h.run_steps(2);
    assert!(h.query_by_label("Opening the Recording").is_none());
    let _ = shown;
}

// ---- the library and the media bin ----

/// A host that brings files into the media bin at once: a red picture, a second of a tone.
#[derive(Clone, Default)]
struct MediaHost {
    answer: std::sync::Arc<std::sync::Mutex<Option<crate::services::DialogResult>>>,
    imports: std::sync::Arc<std::sync::Mutex<usize>>,
}

impl crate::services::Services for MediaHost {
    fn request_import(&self) {
        *self.imports.lock().unwrap() += 1;
    }
    fn poll_dialog(&self) -> Option<crate::services::DialogResult> {
        self.answer.lock().unwrap().take()
    }
    fn append_file(&self, _path: &str, _bytes: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn load_media(&self, path: &str) -> Result<screengrabcraft_media::media_store::MediaJob, String> {
        use screengrabcraft_media::layers::MediaKind;
        use screengrabcraft_media::media_store::{Content, Loaded};
        let loaded = if path.ends_with(".png") {
            let pic = Raster::filled(80, 40, Color::rgb(0xE0, 0x20, 0x20)).unwrap();
            Loaded { kind: MediaKind::Picture, duration: 0, width: 80, height: 40, content: Content::Picture(pic) }
        } else {
            let tone: Vec<i16> = (0..48_000 * 2)
                .flat_map(|i| {
                    let v = ((i as f32 * 0.03).sin() * 12_000.0) as i16;
                    [v, v]
                })
                .collect();
            Loaded { kind: MediaKind::Sound, duration: 2_000_000, width: 0, height: 0, content: Content::Sound(tone) }
        };
        Ok(std::sync::Arc::new(std::sync::Mutex::new(Some(Ok(loaded)))))
    }
}

fn media_harness(host: MediaHost) -> Harness<'static, Editor> {
    let mut ed = Editor::new(Box::new(host), EditorSettings::default());
    let path = Some("C:/Captures/Production.sgvideo".to_string());
    ed.open_file(&egui::Context::default(), OpenedFile { name: "Production.sgvideo".into(), path, bytes: recording() });
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
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

fn shot(h: &mut Harness<'_, Editor>, name: &str) {
    if let Ok(dir) = std::env::var("SGC_SHOTS")
        && let Ok(img) = h.render()
    {
        let _ = img.save(format!("{dir}/{name}.png"));
    }
}

/// How many of a picture's pixels are about this colour.
fn count(pic: &Raster, c: [u8; 3]) -> usize {
    pic.pixels().chunks(4).filter(|p| p[0].abs_diff(c[0]) < 30 && p[1].abs_diff(c[1]) < 30 && p[2].abs_diff(c[2]) < 30).count()
}

#[test]
fn the_library_puts_a_lower_third_on_a_layer_of_its_own_and_its_words_change() {
    let mut h = media_harness(MediaHost::default());
    for label in ["Library", "Media", "Title", "Bar", "Callout", "Fade in from black", "Wipe"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    assert!(h.state().video.as_ref().unwrap().thumbnails_made() >= 6, "the cards show what they look like");
    if let Some(v) = h.state_mut().video.as_mut() {
        v.playhead = 1_000_000;
    }
    h.run_steps(1);
    let before = h.state_mut().video.as_mut().unwrap().frame_at(1_500_000);
    h.get_by_label("Add Bar").click();
    h.run_steps(3);
    shot(&mut h, "video-library");
    {
        let v = h.state_mut().video.as_mut().unwrap();
        let layers = &v.session.timeline.layers;
        assert_eq!(layers.len(), 1, "a layer of its own");
        assert_eq!(layers[0].items.len(), 1);
        assert_eq!(layers[0].items[0].at, 1_000_000, "at the playhead");
        assert!(v.item.is_some(), "and chosen");
        let after = v.frame_at(1_500_000);
        assert_ne!(before.pixels(), after.pixels(), "it's on the picture");
    }
    // Its panel: its words, how it comes and goes, and a way to delete it.
    for label in ["Words", "Comes in", "Goes out", "Delete"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    if let Some(v) = h.state_mut().video.as_mut() {
        let (layer, item) = v.item.unwrap();
        v.session.execute("video.setItem", &serde_json::json!({"layer": layer, "item": item, "title": "Ada Lovelace"})).unwrap();
    }
    h.run_steps(2);
    {
        let v = h.state().video.as_ref().unwrap();
        let it = &v.session.timeline.layers[0].items[0];
        assert!(matches!(&it.what, screengrabcraft_media::layers::What::Text { title, .. } if title == "Ada Lovelace"));
    }
    // Another at the same time on that layer never overlaps it: it goes after.
    {
        let ctx = egui::Context::default();
        let ed = h.state_mut();
        let mut v = ed.video.take().unwrap();
        let layer = v.session.timeline.layers[0].id;
        v.drop_on_timeline(ed, &ctx, &crate::video::LibDrop::Template("lower-line"), Some(layer), 1_200_000);
        ed.video = Some(v);
    }
    h.run_steps(2);
    {
        let v = h.state().video.as_ref().unwrap();
        let items = &v.session.timeline.layers[0].items;
        assert_eq!(items.len(), 2);
        assert!(items[1].at >= items[0].end(), "{} {}", items[1].at, items[0].end());
    }
    // Del takes the chosen item off; Ctrl+Z puts it back.
    h.key_press(egui::Key::Delete);
    h.run_steps(2);
    assert_eq!(h.state().video.as_ref().unwrap().session.timeline.layers[0].items.len(), 1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(2);
    let v = h.state().video.as_ref().unwrap();
    assert_eq!(v.session.timeline.layers[0].items.len(), 2);
    // What's put after the end makes the video longer, to its end.
    let last = v.session.timeline.layers[0].items.iter().map(|i| i.end()).max().unwrap();
    assert_eq!(length(&h), last.max(4_000_000));
}

#[test]
fn media_brought_in_goes_in_the_bin_then_onto_a_layer_of_its_kind() {
    use crate::services::DialogResult;
    use screengrabcraft_media::layers::LayerKind;
    let host = MediaHost::default();
    let mut h = media_harness(host.clone());
    h.get_by_label("Media").click();
    h.run_steps(2);
    h.get_by_label("Import Media…").click();
    h.run_steps(1);
    assert_eq!(*host.imports.lock().unwrap(), 1);
    let picked = vec!["C:/Pictures/Logo.png".to_string(), "C:/Music/Song.mp3".to_string(), "C:/Notes/notes.txt".to_string()];
    *host.answer.lock().unwrap() = Some(DialogResult::Import(picked));
    h.run_steps(4);
    {
        let v = h.state().video.as_ref().unwrap();
        let names: Vec<&str> = v.session.timeline.media.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["Logo.png", "Song.mp3"], "the text file is refused");
    }
    shot(&mut h, "video-media-bin");
    h.get_by_label("Add Logo.png").click();
    h.run_steps(2);
    h.get_by_label("Add Song.mp3").click();
    h.run_steps(3);
    let v = h.state_mut().video.as_mut().unwrap();
    let kinds: Vec<_> = v.session.timeline.layers.iter().map(|l| l.kind).collect();
    assert_eq!(kinds, [LayerKind::Picture, LayerKind::Sound]);
    // The picture is on the video, the sound in it.
    let pic = v.frame_at(500_000);
    assert!(count(&pic, [0xE0, 0x20, 0x20]) > 200, "the logo is on the picture");
    let tl = v.session.timeline.clone();
    let plain = screengrabcraft_media::compose::mix(&v.rec, &tl, 24_000, 4800);
    let with = screengrabcraft_media::compose::mix_with(&v.rec, &tl, v.library.as_ref(), 24_000, 4800);
    assert_ne!(plain, with, "the song is heard");
    // Taken out of the bin, it's taken off the timeline too.
    let id = v.session.timeline.media[0].id;
    v.session.execute("video.removeMedia", &serde_json::json!({"id": id})).unwrap();
    assert!(v.session.timeline.layers[0].items.is_empty());
}

#[test]
fn a_title_dragged_from_the_library_onto_the_timeline_lands_where_it_was_let_go() {
    let mut h = media_harness(MediaHost::default());
    let card = h.get_by_label("Title").rect().center();
    // The band above the tracks (just above the first track's Show): a layer of its own.
    let show = h.query_all_by_label("Show").next().unwrap().rect();
    let to = egui::pos2(show.max.x + 500.0, show.min.y - 32.0);
    crate::tests::drag(&mut h, card, to);
    let v = h.state().video.as_ref().unwrap();
    assert_eq!(v.session.timeline.layers.len(), 1, "dropped on a layer of its own");
    let at = v.session.timeline.layers[0].items[0].at;
    assert!(at > 0, "where it was let go, not at the start: {at}");
}

#[test]
fn a_layer_item_is_moved_along_then_onto_another_layer_then_trimmed_on_the_timeline() {
    let mut h = media_harness(MediaHost::default());
    let place = |h: &mut Harness<'_, Editor>, key: &'static str, at: u64| {
        let ctx = egui::Context::default();
        let ed = h.state_mut();
        let mut v = ed.video.take().unwrap();
        v.drop_on_timeline(ed, &ctx, &crate::video::LibDrop::Template(key), None, at);
        let chosen = v.item.unwrap();
        ed.video = Some(v);
        chosen
    };
    let (a, ia) = place(&mut h, "title", 0);
    let (b, ib) = place(&mut h, "callout", 8_000_000);
    if let Some(v) = h.state_mut().video.as_mut() {
        v.pps = 0.0;
    }
    h.run_steps(3);
    let rect = |h: &Harness<'_, Editor>, l: u32, i: u32| h.state().video.as_ref().unwrap().item_on_timeline(l, i).unwrap();
    let pps = h.state().video.as_ref().unwrap().pps;
    // Along its layer by a second.
    let r = rect(&h, a, ia);
    crate::tests::drag(&mut h, r.center(), r.center() + egui::vec2(pps, 0.0));
    let at = h.state().video.as_ref().unwrap().session.timeline.item(a, ia).unwrap().at;
    assert!((850_000..=1_150_000).contains(&at), "{at}");
    // Onto the other layer: it goes there, and its own layer, left empty, goes.
    let r = rect(&h, a, ia);
    let to = egui::pos2(r.center().x, rect(&h, b, ib).center().y);
    crate::tests::drag(&mut h, r.center(), to);
    {
        let tl = &h.state().video.as_ref().unwrap().session.timeline;
        assert_eq!(tl.layers.len(), 1, "{:?}", tl.layers);
        assert!(tl.item(b, ia).is_some(), "it's on the other layer, keeping its number");
    }
    // Trimmed by its end, a second shorter.
    let len = h.state().video.as_ref().unwrap().session.timeline.item(b, ia).unwrap().len;
    let r = rect(&h, b, ia);
    let edge = egui::pos2(r.max.x - 2.0, r.center().y);
    crate::tests::drag(&mut h, edge, edge - egui::vec2(pps, 0.0));
    let after = h.state().video.as_ref().unwrap().session.timeline.item(b, ia).unwrap().len;
    assert!(len - after > 800_000 && len - after < 1_200_000, "{len} → {after}");
    // Each was one undo step.
    for _ in 0..3 {
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
        h.run_steps(2);
    }
    let tl = &h.state().video.as_ref().unwrap().session.timeline;
    assert_eq!(tl.item(a, ia).map(|i| i.at), Some(0), "back as it was");
}

#[test]
fn the_chosen_title_and_the_camera_are_dragged_and_resized_on_the_picture() {
    let mut h = media_harness(MediaHost::default());
    {
        let ctx = egui::Context::default();
        let ed = h.state_mut();
        let mut v = ed.video.take().unwrap();
        v.playhead = 1_000_000;
        v.drop_on_timeline(ed, &ctx, &crate::video::LibDrop::Template("title-box"), None, 0);
        ed.video = Some(v);
    }
    h.run_steps(3);
    let place = |h: &Harness<'_, Editor>| h.state().video.as_ref().unwrap().session.timeline.layers[0].items[0].place;
    let p0 = place(&h);
    let pic = h.state().video.as_ref().unwrap().picture_rect.unwrap();
    // Dragged by its middle: moved, as one undo step.
    let from = egui::pos2(pic.min.x + pic.width() * p0.x, pic.min.y + pic.height() * p0.y);
    crate::tests::drag(&mut h, from, from + egui::vec2(-pic.width() * 0.2, pic.height() * 0.15));
    let p1 = place(&h);
    assert!(p1.x < p0.x - 0.1 && p1.y > p0.y + 0.08, "{p0:?} → {p1:?}");
    shot(&mut h, "video-designer");
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(2);
    assert_eq!(place(&h).x, p0.x, "one undo step puts it back");
    // The camera: taken hold of on the picture and dragged to the middle.
    if let Some(v) = h.state_mut().video.as_mut() {
        v.item = None;
        let layer = v.session.timeline.layers[0].id;
        v.session.execute("video.deleteLayer", &serde_json::json!({"layer": layer})).unwrap();
    }
    h.run_steps(2);
    // (The timeline is a row shorter now, so the picture has moved.)
    let pic = h.state().video.as_ref().unwrap().picture_rect.unwrap();
    let cam = h.state().video.as_ref().unwrap().camera_on_screen().expect("the camera is on the picture");
    crate::tests::drag(&mut h, cam.center(), pic.center());
    let at = h.state().video.as_ref().unwrap().session.timeline.camera.at.expect("the camera has a place of its own");
    assert!((at[0] - 0.5).abs() < 0.1 && (at[1] - 0.5).abs() < 0.12, "{at:?}");
}
