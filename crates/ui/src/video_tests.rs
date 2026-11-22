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
    harness_stepping(0.25)
}

/// [`harness`], each of its steps so long (seconds): a double click needs them short.
fn harness_stepping(dt: f32) -> Harness<'static, Editor> {
    let mut ed = Editor::new(Box::new(NoServices), EditorSettings::default());
    ed.open_file(&egui::Context::default(), OpenedFile { name: "Production.sgvideo".into(), path: None, bytes: recording() });
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_step_dt(dt).build_ui_state(
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
    for label in ["Export Video", "Show", "Top left", "Circle", "Full Screen (F11)"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    // Splitting and cutting are on the timeline's right button, not on the tool bar.
    for gone in ["Split", "Cut"] {
        assert!(h.query_by_label(gone).is_none(), "{gone} is on the tool bar");
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
    h.key_press(egui::Key::S);
    h.run_steps(2);
    let clips: Vec<usize> = h.state().video.as_ref().unwrap().session.timeline.tracks.iter().map(|t| t.clips.len()).collect();
    assert_eq!(clips, vec![2, 2, 2], "every track split at the playhead");
    if let Some(v) = h.state_mut().video.as_mut() {
        v.range = Some((1_000_000, 2_000_000));
    }
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::X);
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
    /// How long the video made is.
    length: u64,
}

impl crate::services::Services for CaptionHost {
    fn video_formats(&self) -> Vec<crate::services::VideoFormat> {
        use crate::services::VideoFormat;
        vec![VideoFormat::Mp4, VideoFormat::Mp4Av1, VideoFormat::Gif, VideoFormat::Wav]
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
            length: job.timeline.length(),
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
    caption_harness_stepping(bytes, host, 0.25)
}

/// [`caption_harness`], each of its steps so long (seconds): a double click needs them short.
fn caption_harness_stepping(bytes: Vec<u8>, host: CaptionHost, dt: f32) -> Harness<'static, Editor> {
    let mut ed = Editor::new(Box::new(host), EditorSettings::default());
    let path = Some("C:/Captures/Production.sgvideo".to_string());
    ed.open_file(&egui::Context::default(), OpenedFile { name: "Production.sgvideo".into(), path, bytes });
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 860.0)).with_step_dt(dt).build_ui_state(
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
        Job {
            out: "D:/Videos/Demo.mp4".into(),
            replace: true,
            format: VideoFormat::Mp4,
            captions: Some(2),
            burn: false,
            subtitles: true,
            length: 4_000_000
        }
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
fn just_the_chosen_stretch_is_made_with_its_own_captions_and_just_the_sound_as_a_wav() {
    use crate::services::VideoFormat;
    use crate::video::{CaptionsIn, ExportStep};
    let mut bytes = meeting();
    bytes.extend(screengrabcraft_media::recording::captions_bytes(&captions()).unwrap());
    let host = CaptionHost::default();
    let mut h = caption_harness(bytes, host.clone());
    // A stretch chosen: the window offers it alone, the whole video still as it was.
    if let Some(v) = h.state_mut().video.as_mut() {
        v.choose_stretch(1_000_000, 2_000_000);
    }
    h.run_steps(1);
    h.get_by_label("Export Video").click();
    h.run_steps(2);
    for label in ["What", "The whole video (0:04.0)", "Just the chosen stretch: 0:01.0 to 0:02.0 (0:01.0)"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    h.get_by_label("Just the chosen stretch: 0:01.0 to 0:02.0 (0:01.0)").click();
    h.run_steps(1);
    if let Ok(dir) = std::env::var("SGC_SHOTS")
        && let Ok(img) = h.render()
    {
        let _ = img.save(format!("{dir}/export-stretch.png"));
    }
    h.get_by_label("Export").click();
    h.run_steps(2);
    let job = host.jobs.lock().unwrap().pop().unwrap();
    // A second long; your line (0.2 s to 1.8 s) is in it, the others' (from 2 s) isn't.
    assert_eq!((job.length, job.captions, job.subtitles), (1_000_000, Some(1), true));
    *host.making.lock().unwrap().as_ref().unwrap().done.lock().unwrap() = Some(Ok("C:/Captures/Production.mp4".into()));
    h.run_steps(2);
    h.get_by_label("Close").click();
    h.run_steps(2);
    // File, Export Sound…: just the sound, a WAV here (M4A is greyed out on this host).
    h.get_by_label("File").click();
    h.run_steps(1);
    h.get_by_label("Export Sound…").click();
    h.run_steps(2);
    match &h.state().video.as_ref().unwrap().export {
        Some(ExportStep::Choose(c)) => {
            assert_eq!((c.format, c.captions, c.path.as_str()), (VideoFormat::Wav, CaptionsIn::Off, "C:/Captures/Production.wav"));
        }
        _ => panic!("the Export window is open"),
    }
    // Captions can't go on a picture it hasn't got; beside it, they can.
    h.get_by_label("On the picture (always shown)").click();
    h.run_steps(1);
    match &h.state().video.as_ref().unwrap().export {
        Some(ExportStep::Choose(c)) => assert_eq!(c.captions, CaptionsIn::Off),
        _ => panic!("the Export window is open"),
    }
    if let Ok(dir) = std::env::var("SGC_SHOTS")
        && let Ok(img) = h.render()
    {
        let _ = img.save(format!("{dir}/export-sound.png"));
    }
    h.get_by_label("Export").click();
    h.run_steps(2);
    let job = host.jobs.lock().unwrap().pop().unwrap();
    assert_eq!((job.format, job.burn, job.length, job.out.as_str()), (VideoFormat::Wav, false, 4_000_000, "C:/Captures/Production.wav"));
    *host.making.lock().unwrap().as_ref().unwrap().done.lock().unwrap() = Some(Ok("C:/Captures/Production.wav".into()));
    h.run_steps(2);
    for label in ["Play Sound", "Show in Folder"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    h.get_by_label("Play Sound").click();
    h.run_steps(2);
    assert_eq!(host.shown.lock().unwrap().last().map(String::as_str), Some("open:C:/Captures/Production.wav"));
    // A name ending .wav picked for a video makes the sound; .mp4 a video again.
    h.get_by_label("Export Video").click();
    h.run_steps(2);
    if let Some(v) = h.state_mut().video.as_mut() {
        v.export_saved_to("D:/Sound/Talk.wav");
    }
    match &h.state().video.as_ref().unwrap().export {
        Some(ExportStep::Choose(c)) => assert_eq!((c.format, c.path.as_str()), (VideoFormat::Wav, "D:/Sound/Talk.wav")),
        _ => panic!("the Export window is open"),
    }
    if let Some(v) = h.state_mut().video.as_mut() {
        v.export_saved_to("D:/Sound/Talk.mp4");
    }
    match &h.state().video.as_ref().unwrap().export {
        Some(ExportStep::Choose(c)) => assert_eq!(c.format, VideoFormat::Mp4),
        _ => panic!("the Export window is open"),
    }
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    // The chosen stretch's panel has Export the Stretch…: the window set to make just it.
    h.get_by_label("Export the Stretch…").click();
    h.run_steps(2);
    match &h.state().video.as_ref().unwrap().export {
        Some(ExportStep::Choose(c)) => assert!(c.only_stretch && c.stretch == Some((1_000_000, 2_000_000))),
        _ => panic!("the Export window is open"),
    }
}

/// A host that saves pictures in a made-up captures folder, and has the Image Editor's window
/// beside this one.
#[derive(Clone, Default)]
struct FrameHost {
    written: std::sync::Arc<std::sync::Mutex<Vec<(String, Vec<u8>)>>>,
    handed: std::sync::Arc<std::sync::Mutex<Vec<(String, bool)>>>,
}

impl crate::services::Services for FrameHost {
    fn new_picture_path(&self, source: &str) -> Result<String, String> {
        Ok(format!("C:/Captures/{source}.png"))
    }
    fn write_file(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        self.written.lock().unwrap().push((path.to_string(), bytes.to_vec()));
        Ok(())
    }
    fn append_file(&self, _path: &str, _bytes: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn open_in_other_editor(&self, path: &str, video: bool) -> bool {
        self.handed.lock().unwrap().push((path.to_string(), video));
        true
    }
}

#[test]
fn a_frame_is_saved_as_a_picture_and_opened_in_the_image_editor() {
    let host = FrameHost::default();
    let mut ed = Editor::new(Box::new(host.clone()), EditorSettings::default());
    ed.open_file(&egui::Context::default(), OpenedFile { name: "Production.sgvideo".into(), path: None, bytes: recording() });
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_step_dt(0.25).build_ui_state(
        |ui, ed: &mut Editor| {
            let ctx = ui.ctx().clone();
            ed.logic(&ctx);
            ed.ui(ui);
        },
        ed,
    );
    h.run_steps(3);
    // At 1.5 s the box is at x = 90 to 140.
    if let Some(v) = h.state_mut().video.as_mut() {
        v.playhead = 1_500_000;
    }
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::S);
    h.run_steps(2);
    let (path, png) = host.written.lock().unwrap().pop().expect("a picture was written");
    assert_eq!(path, "C:/Captures/Production 0.01.5.png");
    let pic = screengrabcraft_render::decode(&png).unwrap();
    assert_eq!((pic.width(), pic.height()), (320, 180), "at the recording's own size");
    let p = pic.pixel(110, 60).unwrap();
    assert_eq!((p.r, p.g, p.b), (0x4E, 0x3C, 0xA8), "the box where it was then");
    assert_eq!(pic.pixel(20, 60).unwrap(), Color::WHITE);
    assert_eq!(host.handed.lock().unwrap().as_slice(), [(path.clone(), false)], "handed to the Image Editor");
    assert!(h.state().toast_text().is_some_and(|t| t.contains("opened in the Image Editor")), "{:?}", h.state().toast_text());
    // From the ruler's right button, at that moment: three quarters of the way along.
    let (lane, _) = h.state().video.as_ref().unwrap().lane_rect.unwrap();
    right_click(&mut h, egui::pos2(lane.min.x + (lane.width() - 24.0) * 0.75, lane.min.y + 8.0));
    h.get_by_label("Save This Frame as a Picture").click();
    h.run_steps(2);
    let (path, _) = host.written.lock().unwrap().pop().expect("a second picture was written");
    assert!(path.starts_with("C:/Captures/Production 0.0"), "{path}");
    assert!(h.state().video.as_ref().unwrap().playhead.abs_diff(3_000_000) < 150_000, "the playhead went there");
}

#[test]
fn h_holds_the_frame_its_panel_holds_it_longer_and_takes_it_out() {
    let mut h = harness();
    if let Some(v) = h.state_mut().video.as_mut() {
        v.playhead = 1_500_000;
    }
    h.run_steps(1);
    h.key_press(egui::Key::H);
    h.run_steps(2);
    assert_eq!(length(&h), 6_000_000, "two seconds held, everything after moved along");
    {
        let v = h.state().video.as_ref().unwrap();
        let (t, c) = v.selected.expect("the held frame is chosen");
        let clip = v.session.timeline.tracks[t].clips[c];
        assert!(clip.hold && clip.at == 1_500_000 && clip.len == 2_000_000);
        // The screen (and the camera) hold the picture, the microphone waits.
        let mic = v.session.timeline.find(StreamKind::Microphone).unwrap();
        assert_eq!(v.session.timeline.source_time(0, 3_000_000), Some(1_500_000));
        assert_eq!(v.session.timeline.source_time(mic, 3_000_000), None);
        assert_eq!(v.session.timeline.source_time(mic, 3_600_000), Some(1_600_000));
    }
    assert!(h.query_all_by_label("Held frame").next().is_some());
    shot(&mut h, "video-held-frame");
    // Held 3 s from the panel; the picture under it is the moment held.
    h.get_by_label("3 s").click();
    h.run_steps(2);
    assert_eq!(length(&h), 7_000_000);
    if let Some(v) = h.state_mut().video.as_mut() {
        v.playhead = 4_000_000;
    }
    h.run_steps(2);
    // Undone, then taken out from the panel: as it was.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(2);
    assert_eq!(length(&h), 6_000_000);
    if let Some(v) = h.state_mut().video.as_mut() {
        let (t, c) = (0, v.session.timeline.tracks[0].clips.iter().position(|c| c.hold).unwrap());
        v.selected = Some((t, c));
    }
    h.run_steps(2);
    h.get_by_label("Take the Hold Out").click();
    h.run_steps(2);
    assert_eq!(length(&h), 4_000_000);
    assert!(h.state().video.as_ref().unwrap().session.timeline.tracks.iter().all(|t| t.clips.iter().all(|c| !c.hold)));
    // The ruler's right button holds the frame there, from its menu.
    let (lane, _) = h.state().video.as_ref().unwrap().lane_rect.unwrap();
    right_click(&mut h, egui::pos2(lane.min.x + (lane.width() - 24.0) * 0.5, lane.min.y + 8.0));
    h.get_by_label_contains("Hold This Frame").hover();
    h.run_steps(2);
    h.get_by_label("For 1 Second").click();
    h.run_steps(2);
    assert_eq!(length(&h), 5_000_000);
}

#[test]
fn captions_can_be_asked_for_and_appear_when_theyre_made() {
    let host = CaptionHost::default();
    let mut h = caption_harness(meeting(), host.clone());
    assert!(h.state().video.as_ref().unwrap().captions.is_none());
    h.get_by_label("Transcribe").click();
    h.run_steps(2);
    assert_eq!(host.asked.lock().unwrap().as_slice(), ["C:/Captures/Production.sgvideo"]);
    assert!(h.state().video.as_ref().unwrap().captions_coming.is_some());
    assert!(h.query_all_by_label("Transcribe").next().is_none(), "asked once");
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
    h.get_by_label("Transcribe").click();
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
/// It transcribes too, when told: the files asked for (`again:` before those heard again), how
/// far each has got, the recording's captions and the media's lines once they're made.
#[derive(Clone, Default)]
struct MediaHost {
    answer: std::sync::Arc<std::sync::Mutex<Option<crate::services::DialogResult>>>,
    imports: std::sync::Arc<std::sync::Mutex<usize>>,
    auto: bool,
    asked: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    progress: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Result<f32, String>>>>,
    ready: std::sync::Arc<std::sync::Mutex<Option<screengrabcraft_media::captions::Captions>>>,
    media_lines: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, (u64, Vec<screengrabcraft_media::captions::Cue>)>>>,
}

impl crate::services::Services for MediaHost {
    fn request_import(&self) {
        *self.imports.lock().unwrap() += 1;
    }
    fn auto_captions(&self) -> bool {
        self.auto
    }
    fn make_captions(&self, path: &str) -> Result<(), String> {
        self.asked.lock().unwrap().push(path.to_string());
        Ok(())
    }
    fn make_captions_again(&self, path: &str) -> Result<(), String> {
        self.asked.lock().unwrap().push(format!("again:{path}"));
        Ok(())
    }
    fn captions_progress(&self, path: &str) -> Option<Result<f32, String>> {
        self.progress.lock().unwrap().get(path).cloned()
    }
    fn poll_captions(&self, _recording: &str, seen: u64) -> Option<(u64, Option<screengrabcraft_media::captions::Captions>)> {
        let ready = self.ready.lock().unwrap().clone();
        (ready.is_some() && seen != 7).then_some((7, ready))
    }
    fn poll_media_captions(&self, path: &str, seen: u64) -> Option<(u64, Vec<screengrabcraft_media::captions::Cue>)> {
        let (at, lines) = self.media_lines.lock().unwrap().get(path).cloned()?;
        (at != seen).then_some((at, lines))
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
    for label in [
        "Library",
        "Media",
        "Lower Thirds",
        "News and Broadcast",
        "Titles",
        "Callouts and Overlays",
        "News Transitions",
        "Training Transitions",
        "More Transitions",
        "Breaking news",
        "Name card",
        "Angled",
    ] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    assert!(h.state().video.as_ref().unwrap().thumbnails_made() >= 6, "the cards show what they look like");
    // A card is the whole video with the item where it will be and as large: the Name card at the
    // bottom left of it, the rest the screen under it.
    let card = crate::video::VideoView::template_card_picture("news-name").unwrap();
    let screen = crate::video::VideoView::template_card_picture("blank-for-the-test");
    assert!(screen.is_none(), "no such template");
    let (w, h_px) = (card.width() as usize, card.height() as usize);
    let dark = |x0: usize, y0: usize, x1: usize, y1: usize| {
        let mut n = 0;
        for y in y0..y1 {
            for x in x0..x1 {
                let p = &card.pixels()[(y * w + x) * 4..(y * w + x) * 4 + 3];
                n += usize::from(p.iter().all(|c| *c < 60));
            }
        }
        n
    };
    assert!(dark(0, h_px * 3 / 4, w / 3, h_px) > 80, "its dark card at the bottom left");
    assert!(dark(w / 2, 0, w, h_px / 2) < 20, "and nothing of it at the top right");
    // The pointer on a card: a larger preview of it plays.
    let name_card = h.get_by_label("Name card").rect().center();
    h.hover_at(name_card);
    h.run_steps(8);
    assert_eq!(h.state().video.as_ref().unwrap().previewing(), Some("news-name"));
    shot(&mut h, "video-library-preview");
    h.hover_at(egui::pos2(700.0, 300.0));
    h.run_steps(2);
    assert_eq!(h.state().video.as_ref().unwrap().previewing(), None);
    if let Some(v) = h.state_mut().video.as_mut() {
        v.playhead = 1_000_000;
    }
    h.run_steps(1);
    let before = h.state_mut().video.as_mut().unwrap().frame_at(1_500_000);
    h.get_by_label("Add Name card").click();
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
    for label in ["Text", "Entrance", "Exit", "Delete"] {
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
    let card = h.get_by_label("Breaking news").rect().center();
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

// ---- transcribing what's in the video ----

#[test]
fn transcribing_hears_the_sound_put_on_the_timeline_too_and_shows_how_far_it_has_got() {
    use crate::services::DialogResult;
    use screengrabcraft_media::captions::{Cue, Speaker};
    const REC: &str = "C:/Captures/Production.sgvideo";
    const SONG: &str = "C:/Music/Song.mp3";
    let host = MediaHost::default();
    let mut h = media_harness(host.clone());
    // A song brought in and put on the timeline at 1 s.
    *host.answer.lock().unwrap() = Some(DialogResult::Import(vec![SONG.to_string()]));
    h.run_steps(3);
    if let Some(v) = h.state_mut().video.as_mut() {
        v.playhead = 1_000_000;
    }
    h.get_by_label("Media").click();
    h.run_steps(2);
    h.get_by_label("Add Song.mp3").click();
    h.run_steps(2);
    assert!(host.asked.lock().unwrap().is_empty(), "not transcribed by itself unless Settings says so");
    // Transcribe: the recording and the song, each heard on its own.
    h.get_by_label("Transcribe").click();
    h.run_steps(2);
    assert_eq!(host.asked.lock().unwrap().as_slice(), [REC, SONG]);
    // How far it has got, in the captions' header and the status bar.
    host.progress.lock().unwrap().insert(REC.to_string(), Ok(0.5));
    h.state_mut().video.as_mut().unwrap().poll_now();
    h.run_steps(2);
    assert_eq!(h.state().video.as_ref().unwrap().transcribing(), Some(("Transcribing…".to_string(), Some(0.5))));
    assert!(h.query_by_label("Transcribing… 50%").is_some(), "the status bar says so");
    shot(&mut h, "video-transcribing");
    // The song's lines come (in its own time): they're placed where it is on the timeline.
    let line = |s, e, sp, t: &str| Cue { start: s, end: e, speaker: sp, text: t.to_string() };
    host.media_lines.lock().unwrap().insert(SONG.to_string(), (5, vec![line(500_000, 1_500_000, Speaker::Media, "From the song.")]));
    // Then the recording's.
    host.progress.lock().unwrap().clear();
    *host.ready.lock().unwrap() =
        Some(screengrabcraft_media::captions::Captions { cues: vec![line(200_000, 900_000, Speaker::You, "Here it is.")], model: "test".into() });
    h.state_mut().video.as_mut().unwrap().poll_now();
    h.run_steps(3);
    {
        let v = h.state().video.as_ref().unwrap();
        assert!(v.transcribing().is_none());
        let said: Vec<(u64, &str)> = v.cues.iter().map(|c| (c.start, c.text.as_str())).collect();
        assert_eq!(said, [(200_000, "Here it is."), (1_500_000, "From the song.")], "the song's line where the song plays");
    }
    // Transcribe Again: both heard again.
    h.get_by_label("Transcribe Again").click();
    h.run_steps(2);
    assert_eq!(host.asked.lock().unwrap()[2..], [format!("again:{REC}"), format!("again:{SONG}")]);
    // A file that couldn't be heard says why, and isn't waited for.
    host.progress.lock().unwrap().insert(SONG.to_string(), Err("it has no sound".into()));
    h.state_mut().video.as_mut().unwrap().poll_now();
    h.run_steps(2);
    assert!(h.state().video.as_ref().unwrap().media_coming.is_empty());
}

#[test]
fn with_the_setting_on_sound_brought_in_is_transcribed_by_itself() {
    use crate::services::DialogResult;
    let host = MediaHost { auto: true, ..MediaHost::default() };
    let mut h = media_harness(host.clone());
    *host.answer.lock().unwrap() = Some(DialogResult::Import(vec!["C:/Music/Song.mp3".to_string(), "C:/Pictures/Logo.png".to_string()]));
    h.run_steps(3);
    assert_eq!(host.asked.lock().unwrap().as_slice(), ["C:/Music/Song.mp3"], "the song, not the picture");
    assert!(h.state().video.as_ref().unwrap().transcribing().is_some_and(|(t, _)| t == "Transcribing Song.mp3…"));
}

// ---- the mouse pointer ----

/// A made-up recording of a white screen with the pointer crossing it (a red square for its
/// picture) and a click half-way.
fn recording_with_pointer() -> Vec<u8> {
    use screengrabcraft_media::pointer::{LEFT, SHOWING, Sample, Shape};
    let streams = vec![StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 320, height: 180, channels: 0 }];
    let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
    w.pointer_shape(&Shape { id: 1, hot: (0, 0), nominal: 12, pic: Raster::filled(12, 12, Color::rgb(0xE0, 0x20, 0x20)).unwrap() }).unwrap();
    for t in 0..40u64 {
        w.video_frame(0, t * 100_000, &Raster::filled(320, 180, Color::WHITE).unwrap()).unwrap();
        let flags = SHOWING | if t == 20 { LEFT } else { 0 };
        w.pointer(&Sample { t: t * 100_000, x: 20.0 + t as f32 * 6.0, y: 60.0, flags, shape: 1 }).unwrap();
    }
    w.finish(4_000_000).unwrap()
}

#[test]
fn the_recorded_pointer_shows_on_the_video_and_is_enlarged_or_hidden_from_its_panel() {
    let mut ed = Editor::new(Box::new(NoServices), EditorSettings::default());
    ed.open_file(&egui::Context::default(), OpenedFile { name: "Pointer.sgvideo".into(), path: None, bytes: recording_with_pointer() });
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).build_ui_state(
        |ui, ed: &mut Editor| {
            let ctx = ui.ctx().clone();
            ed.logic(&ctx);
            ed.ui(ui);
        },
        ed,
    );
    h.run_steps(3);
    for label in ["Pointer", "Show the mouse pointer", "Size", "Smoother movement", "Show clicks"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    if let Some(v) = h.state_mut().video.as_mut() {
        v.playhead = 1_000_000;
    }
    h.run_steps(2);
    shot(&mut h, "video-pointer");
    let red = |h: &mut Harness<'_, Editor>| count(&h.state_mut().video.as_mut().unwrap().frame_at(1_000_000), [0xE0, 0x20, 0x20]);
    let as_recorded = red(&mut h);
    assert!(as_recorded >= 100, "the pointer is on the video: {as_recorded}");
    // Larger (from the engine, as its slider does).
    h.state_mut().video.as_mut().unwrap().session.execute("video.pointer", &serde_json::json!({"size": 3.0})).unwrap();
    assert!(red(&mut h) > as_recorded * 6);
    // Unticked: it isn't on the video; one undo step brings it back.
    h.get_by_label("Show the mouse pointer").click();
    h.run_steps(2);
    assert!(!h.state().video.as_ref().unwrap().session.timeline.pointer.show);
    assert_eq!(red(&mut h), 0);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(2);
    assert!(red(&mut h) > 0);
    // A recording without a pointer has no such panel.
    let h = harness();
    assert!(h.query_by_label("Show the mouse pointer").is_none());
}

/// A white screen and a camera of a green screen with a red box (the person) in it, for 2 s.
fn recording_green_screen() -> Vec<u8> {
    let streams = vec![
        StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 320, height: 180, channels: 0 },
        StreamInfo { id: 3, kind: StreamKind::Camera, name: "Camera".into(), width: 80, height: 60, channels: 0 },
    ];
    let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
    let mut cam = image::RgbImage::from_pixel(80, 60, image::Rgb([0x00, 0xB1, 0x40]));
    for y in 10..60 {
        for x in 30..50 {
            cam.put_pixel(x, y, image::Rgb([0xE0, 0x20, 0x20]));
        }
    }
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95).encode_image(&cam).unwrap();
    for t in 0..20u64 {
        w.video_frame(0, t * 100_000, &Raster::filled(320, 180, Color::WHITE).unwrap()).unwrap();
        w.jpeg_frame(3, t * 100_000, &jpeg).unwrap();
    }
    w.finish(2_000_000).unwrap()
}

/// A made-up background model: the red is the person.
struct RedIsThePerson;

impl screengrabcraft_render::matte::Matting for RedIsThePerson {
    fn name(&self) -> &str {
        "test"
    }
    fn matte(
        &self,
        picture: &Raster,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<screengrabcraft_render::matte::Matte, screengrabcraft_render::matte::MatteError> {
        progress(0.5);
        let alpha = picture.pixels().chunks(4).map(|p| if p[0] > 150 && p[1] < 100 { 255 } else { 0 }).collect();
        Ok(screengrabcraft_render::matte::Matte::new(picture.width(), picture.height(), alpha).unwrap())
    }
}

/// A host with the made-up model, keeping what's added to the recording file.
#[derive(Clone, Default)]
struct ModelHost(std::sync::Arc<std::sync::Mutex<Vec<(String, usize)>>>);

impl crate::services::Services for ModelHost {
    fn background_model(&self) -> Result<std::sync::Arc<dyn screengrabcraft_render::matte::Matting>, String> {
        Ok(std::sync::Arc::new(RedIsThePerson))
    }
    fn append_file(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        self.0.lock().unwrap().push((path.to_string(), bytes.len()));
        Ok(())
    }
}

#[test]
fn the_cameras_green_screen_or_its_background_is_taken_away_from_its_panel() {
    let host = ModelHost::default();
    let mut ed = Editor::new(Box::new(host.clone()), EditorSettings::default());
    let path = Some("C:/Captures/Green.sgvideo".to_string());
    ed.open_file(&egui::Context::default(), OpenedFile { name: "Green.sgvideo".into(), path, bytes: recording_green_screen() });
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 900.0)).with_step_dt(0.25).build_ui_state(
        |ui, ed: &mut Editor| {
            let ctx = ui.ctx().clone();
            ed.logic(&ctx);
            ed.ui(ui);
        },
        ed,
    );
    h.run_steps(3);
    let green = |h: &mut Harness<'_, Editor>| count(&h.state_mut().video.as_mut().unwrap().frame_at(500_000), [0x00, 0xB1, 0x40]);
    let red = |h: &mut Harness<'_, Editor>| count(&h.state_mut().video.as_mut().unwrap().frame_at(500_000), [0xE0, 0x20, 0x20]);
    let (green_before, red_before) = (green(&mut h), red(&mut h));
    assert!(green_before > 400 && red_before > 100, "{green_before} {red_before}");
    shot(&mut h, "video-camera-panel");
    // Remove a colour: the green screen goes, the person stays.
    h.get_by_value("Keep it").click();
    h.run_steps(3);
    h.get_by_label("Remove a colour (green screen)").click();
    h.run_steps(2);
    assert_eq!(h.state().video.as_ref().unwrap().session.timeline.camera.remove, screengrabcraft_media::keyer::Remove::Colour);
    assert!(green(&mut h) < 20, "{}", green(&mut h));
    assert!(red(&mut h) > red_before / 2);
    assert!(h.query_by_label("Tolerance").is_some());
    shot(&mut h, "video-camera-green-screen");
    // Remove the background instead: it's learnt on a thread, then kept in the file.
    h.get_by_value("Remove a colour (green screen)").click();
    h.run_steps(3);
    h.get_by_label("Remove it (no green screen)").click();
    for _ in 0..40 {
        h.run_steps(1);
        if h.state().video.as_ref().unwrap().rec.plate().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    h.run_steps(2);
    assert!(h.state().video.as_ref().unwrap().rec.plate().is_some(), "the background was learnt");
    assert_eq!(host.0.lock().unwrap().last().map(|(p, _)| p.as_str()), Some("C:/Captures/Green.sgvideo"));
    assert!(green(&mut h) < 60, "{}", green(&mut h));
    assert!(red(&mut h) > red_before / 2);
    assert!(h.query_by_label("Learn It Again").is_some());
    shot(&mut h, "video-camera-background");
    // Undone: kept as it was.
    for _ in 0..2 {
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
        h.run_steps(2);
    }
    assert!(green(&mut h) > 400);
}

/// A grey screen for 4 s, with Ctrl+S pressed at 1 s and Enter at 3 s.
fn recording_with_keys() -> Vec<u8> {
    let streams = vec![StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 320, height: 180, channels: 0 }];
    let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
    for t in 0..40u64 {
        w.video_frame(0, t * 100_000, &Raster::filled(320, 180, Color::rgb(0x80, 0x80, 0x80)).unwrap()).unwrap();
    }
    for (t, keys) in [(1_000_000, "Ctrl+S"), (3_000_000, "Enter")] {
        w.key_press(&screengrabcraft_media::keys::Press { t, keys: keys.to_string() }).unwrap();
    }
    w.finish(4_000_000).unwrap()
}

#[test]
fn the_shortcuts_pressed_while_recording_are_shown_as_keys_from_their_panel() {
    let mut ed = Editor::new(Box::new(NoServices), EditorSettings::default());
    ed.open_file(&egui::Context::default(), OpenedFile { name: "Keys.sgvideo".into(), path: None, bytes: recording_with_keys() });
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).build_ui_state(
        |ui, ed: &mut Editor| {
            let ctx = ui.ctx().clone();
            ed.logic(&ctx);
            ed.ui(ui);
        },
        ed,
    );
    h.run_steps(3);
    let grey = |h: &mut Harness<'_, Editor>, at: u64| count(&h.state_mut().video.as_mut().unwrap().frame_at(at), [0x80, 0x80, 0x80]);
    let whole = 320 * 180;
    assert_eq!(grey(&mut h, 1_200_000), whole, "not shown until asked for");
    h.get_by_label("Show the keys pressed").click();
    h.run_steps(2);
    assert!(h.state().video.as_ref().unwrap().session.timeline.keys.show);
    // Just after Ctrl+S the keys are on the picture; a second and a half later they've gone.
    assert!(grey(&mut h, 1_200_000) < whole - 300);
    assert_eq!(grey(&mut h, 2_700_000), whole);
    if let Some(v) = h.state_mut().video.as_mut() {
        v.playhead = 1_200_000;
    }
    h.run_steps(2);
    shot(&mut h, "video-keys");
    // Moved to the top: the bottom half is grey again.
    if let Some(v) = h.state_mut().video.as_mut() {
        v.session.execute("video.keys", &serde_json::json!({"place": "top"})).unwrap();
    }
    let pic = h.state_mut().video.as_mut().unwrap().frame_at(1_200_000);
    let bottom = pic.pixels().chunks(4).skip(320 * 90).filter(|p| p[0] != 0x80).count();
    assert_eq!(bottom, 0);
    // A recording that kept none says so.
    let h = harness();
    assert!(h.query_by_label("Show the keys pressed").is_none());
    assert!(h.query_by_label_contains("No shortcuts were kept").is_some());
}

#[test]
fn the_pointer_is_spotlit_magnified_highlighted_and_its_clicks_heard_from_its_panel() {
    let mut ed = Editor::new(Box::new(NoServices), EditorSettings::default());
    ed.open_file(&egui::Context::default(), OpenedFile { name: "Pointer.sgvideo".into(), path: None, bytes: recording_with_pointer() });
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 900.0)).build_ui_state(
        |ui, ed: &mut Editor| {
            let ctx = ui.ctx().clone();
            ed.logic(&ctx);
            ed.ui(ui);
        },
        ed,
    );
    h.run_steps(3);
    // At 1 s the pointer is at (80, 60) on the white screen.
    h.state_mut().video.as_mut().unwrap().playhead = 1_000_000;
    let frame = |h: &mut Harness<'_, Editor>| h.state_mut().video.as_mut().unwrap().frame_at(1_000_000);
    let white = frame(&mut h);
    assert_eq!(white.pixel(300, 170).unwrap(), Color::WHITE);
    for label in ["Spotlight", "Magnifier", "Highlight", "Click sounds"] {
        h.get_by_label(label).click();
        h.run_steps(2);
    }
    let look = h.state().video.as_ref().unwrap().session.timeline.pointer;
    assert!(look.spotlight && look.magnify && look.highlight && look.click_sound);
    let lit = frame(&mut h);
    assert!(lit.pixel(300, 170).unwrap().r < 150, "dimmed far from the pointer");
    let near = lit.pixel(70, 60).unwrap();
    assert!(near.b < near.r, "yellow under it: {near:?}");
    shot(&mut h, "video-pointer-effects");
    // The click at 2 s is heard.
    let v = h.state().video.as_ref().unwrap();
    let click = screengrabcraft_media::compose::mix(&v.rec, &v.session.timeline, 96_000, 1440);
    assert!(click.iter().any(|s| s.abs() > 0.1));
    // Four undo steps: as it was.
    for _ in 0..4 {
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
        h.run_steps(1);
    }
    assert_eq!(frame(&mut h).pixel(300, 170).unwrap(), Color::WHITE);
}

// ---- the broadcast graphics ----

#[test]
fn a_breaking_news_lower_third_is_added_played_and_moved_on_the_picture() {
    let mut h = media_harness(MediaHost::default());
    for label in ["Lower Thirds", "Breaking news", "Headline band", "Name card", "Angled", "Minimal"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    // The other sections are shut until their headings are pressed.
    assert!(h.query_by_label("Clock wipe").is_none() && h.query_by_label("Keys").is_none());
    h.get_by_label("Lower Thirds").click();
    h.run_steps(8);
    h.get_by_label("Training Transitions").click();
    h.run_steps(8);
    for label in ["Chapter card", "Next step", "Progress bar", "Done"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    h.get_by_label("Training Transitions").click();
    h.run_steps(8);
    assert!(h.query_by_label("Chapter card").is_none(), "shut again");
    h.get_by_label("Lower Thirds").click();
    h.run_steps(8);
    h.get_by_label("Add Breaking news").click();
    h.run_steps(3);
    let red = |h: &mut Harness<'_, Editor>, t: u64| {
        let pic = h.state_mut().video.as_mut().unwrap().frame_at(t);
        pic.pixels().chunks(4).filter(|p| p[0] > 140 && p[1] < 70 && p[2] < 90).count()
    };
    // It comes on, stays, and goes with its item (6 s from the playhead at 0).
    assert_eq!(red(&mut h, 0), 0, "not there at its very start");
    let held = red(&mut h, 3_000_000);
    assert!(held > 60, "its red tab is on the picture: {held}");
    assert!(red(&mut h, 5_990_000) < held / 3, "all but gone by its end");
    // Its words are changed in its panel (through the engine, as the panel does).
    for label in ["Text", "Colour", "Delete"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    if let Some(v) = h.state_mut().video.as_mut() {
        v.playhead = 3_000_000;
        let (layer, item) = v.item.unwrap();
        v.session
            .execute(
                "video.setItem",
                &serde_json::json!({"layer": layer, "item": item, "title": "Storm warning for the coast", "subtitle": "Winds to 70 mph"}),
            )
            .unwrap();
    }
    h.run_steps(3);
    shot(&mut h, "video-news");
    // Dragged on the picture: it moves by as far as it was dragged.
    let (pic, on) = {
        let v = h.state().video.as_ref().unwrap();
        (v.picture_rect.unwrap(), v.chosen_on_screen().expect("it's chosen and on the picture"))
    };
    let before = h.state().video.as_ref().unwrap().session.timeline.layers[0].items[0].place;
    crate::tests::drag(&mut h, on.center(), on.center() + egui::vec2(pic.width() * 0.25, -pic.height() * 0.2));
    let after = h.state().video.as_ref().unwrap().session.timeline.layers[0].items[0].place;
    assert!((after.x - before.x - 0.25).abs() < 0.03 && (after.y - before.y + 0.2).abs() < 0.03, "{before:?} → {after:?}");
}

#[test]
fn an_item_is_dragged_off_the_picture_and_taken_hold_of_again_out_there() {
    let mut h = media_harness(MediaHost::default());
    {
        let ctx = egui::Context::default();
        let ed = h.state_mut();
        let mut v = ed.video.take().unwrap();
        v.playhead = 2_000_000;
        v.drop_on_timeline(ed, &ctx, &crate::video::LibDrop::Template("news-name"), None, 0);
        ed.video = Some(v);
    }
    h.run_steps(3);
    let (pic, on) = {
        let v = h.state().video.as_ref().unwrap();
        (v.picture_rect.unwrap(), v.chosen_on_screen().unwrap())
    };
    // Off the left edge of the picture, all of it.
    crate::tests::drag(&mut h, on.center(), egui::pos2(pic.min.x - on.width(), on.center().y));
    let out = h.state().video.as_ref().unwrap().chosen_on_screen().unwrap();
    assert!(out.max.x < pic.min.x + 2.0, "it is off the picture: {out:?} {pic:?}");
    shot(&mut h, "video-off-picture");
    // Out there it is still taken hold of, and brought back.
    crate::tests::drag(&mut h, out.center(), pic.center());
    let back = h.state().video.as_ref().unwrap().chosen_on_screen().unwrap();
    assert!((back.center().x - pic.center().x).abs() < 6.0 && (back.center().y - pic.center().y).abs() < 6.0, "{back:?}");
    // The camera too: put where it is part off the picture, it is drawn where it is on it.
    let v = h.state_mut().video.as_mut().unwrap();
    v.session.execute("video.cameraAt", &serde_json::json!({"x": 1.0, "y": 0.5})).unwrap();
    assert_eq!(v.session.timeline.camera.at, Some([1.0, 0.5]));
    let _ = v.frame_at(2_000_000);
}

#[test]
fn the_preview_goes_over_the_whole_screen_and_comes_back() {
    let mut h = harness();
    let full = |h: &Harness<'_, Editor>| h.state().video.as_ref().unwrap().full_screen;
    // The button beside the ones that play, under the picture.
    let (play, button) = (h.get_by_label("Play (Space)").rect(), h.get_by_label("Full Screen (F11)").rect());
    assert!((play.center().y - button.center().y).abs() < 1.0, "in the same row: {play:?} {button:?}");
    assert!(button.min.x > play.max.x && button.min.x - play.max.x < 120.0, "just after the play buttons: {play:?} {button:?}");
    h.get_by_label("Full Screen (F11)").click();
    h.run_steps(3);
    assert!(full(&h));
    assert!(h.state().window_full, "the window follows");
    // Nothing but the picture and its controls.
    for gone in ["Export Video", "Show", "Top left"] {
        assert!(h.query_by_label(gone).is_none(), "{gone} is still there");
    }
    // It plays there, and the bar seeks along the whole video.
    h.get_by_label("Play (Space)").click();
    h.run_steps(2);
    assert!(h.state().video.as_ref().unwrap().is_playing());
    h.get_by_label("Pause (Space)").click();
    h.run_steps(2);
    assert!(!h.state().video.as_ref().unwrap().is_playing());
    let seek = h.get_by_label("Seek").rect();
    crate::tests::click_at(&mut h, egui::pos2(seek.min.x + seek.width() * 0.5, seek.center().y));
    let (at, length) = {
        let v = h.state().video.as_ref().unwrap();
        (v.playhead, v.session.timeline.length())
    };
    assert!(at.abs_diff(length / 2) < length / 20, "half way along: {at} of {length}");
    shot(&mut h, "video-full-screen");
    // It is a player there: the keys that edit do nothing.
    let rev = h.state().video.as_ref().unwrap().session.revision();
    h.key_press(egui::Key::S);
    h.key_press(egui::Key::Delete);
    h.run_steps(2);
    assert_eq!(h.state().video.as_ref().unwrap().session.revision(), rev);
    // Esc comes back, to everything as it was.
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(!full(&h) && !h.state().window_full);
    assert!(h.query_by_label("Export Video").is_some());
    // F11 goes there too, and the way back is on the bar.
    h.key_press(egui::Key::F11);
    h.run_steps(3);
    assert!(full(&h));
    h.get_by_label("Exit Full Screen (Esc)").click();
    h.run_steps(3);
    assert!(!full(&h) && !h.state().window_full);
    // A double click on the picture comes back as well.
    let mut h = harness_stepping(0.02);
    h.key_press(egui::Key::F);
    h.run_steps(3);
    assert!(full(&h));
    let middle = egui::pos2(640.0, 300.0);
    h.hover_at(middle);
    h.run_steps(1);
    for _ in 0..2 {
        h.drag_at(middle);
        h.drop_at(middle);
    }
    h.run_steps(3);
    assert!(!full(&h));
}

#[test]
fn the_buttons_beside_play_step_a_frame_and_go_to_the_ends() {
    let mut h = harness();
    let at = |h: &Harness<'_, Editor>| h.state().video.as_ref().unwrap().playhead;
    let length = h.state().video.as_ref().unwrap().session.timeline.length();
    // Ten frames a second: a frame is a tenth of one.
    h.get_by_label("Forward One Frame (Right Arrow)").click();
    h.run_steps(2);
    assert_eq!(at(&h), 100_000);
    h.get_by_label("Go to the End (End)").click();
    h.run_steps(2);
    assert_eq!(at(&h), length);
    h.get_by_label("Back One Frame (Left Arrow)").click();
    h.run_steps(2);
    assert_eq!(at(&h), length - 100_000);
    h.get_by_label("Go to the Start (Home)").click();
    h.run_steps(2);
    assert_eq!(at(&h), 0);
    // Playing is under the picture, once: not on the tool bar as well.
    assert_eq!(h.query_all_by_label("Play (Space)").count(), 1);
}

#[test]
fn the_timeline_zooms_by_its_buttons_around_the_playhead_and_fits_again() {
    let mut h = harness();
    let pps = |h: &Harness<'_, Editor>| h.state().video.as_ref().unwrap().pps;
    let fitted = pps(&h);
    h.get_by_label("Zoom the Timeline In (Ctrl++)").click();
    h.run_steps(3);
    assert!((pps(&h) - fitted * 1.25).abs() < 0.01, "{} from {fitted}", pps(&h));
    h.get_by_label("Zoom the Timeline Out (Ctrl+-)").click();
    h.run_steps(3);
    h.get_by_label("Zoom the Timeline Out (Ctrl+-)").click();
    h.run_steps(3);
    assert!(pps(&h) < fitted);
    h.get_by_label("Fit the Whole Video on the Timeline (Ctrl+0)").click();
    h.run_steps(3);
    assert!((pps(&h) - fitted).abs() < 0.01);
    // The keys do the same.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Equals);
    h.run_steps(3);
    assert!(pps(&h) > fitted);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num0);
    h.run_steps(3);
    assert!((pps(&h) - fitted).abs() < 0.01);
}

#[test]
fn the_right_button_on_the_timeline_chooses_what_is_under_it_and_offers_what_can_be_done() {
    let mut h = harness();
    let (lane, top) = h.state().video.as_ref().unwrap().lane_rect.unwrap();
    // On the first track's clip, half way along.
    let p = egui::pos2(lane.min.x + (lane.width() - 24.0) / 2.0, top + 20.0);
    h.hover_at(p);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed, modifiers: egui::Modifiers::NONE });
    }
    h.run_steps(3);
    assert_eq!(h.state().video.as_ref().unwrap().selected, Some((0, 0)), "the clip under it is chosen");
    h.get_by_label_contains("Screen at 0:0");
    for label in
        ["Split Every Track Here", "Select This Clip's Stretch", "Delete the Clip, Leaving a Gap", "Move the Playhead Here", "Hide This Track"]
    {
        h.get_by_label_contains(label);
    }
    // Nothing chosen yet: the stretch's ways are there, but greyed.
    assert!(egui_kittest::kittest::NodeT::accesskit_node(&h.get_by_label("Remove the Selection from This Track")).is_disabled());
    shot(&mut h, "video-track-menu");
    // Split This Track Here: the playhead goes there, and only this track's clip is two.
    h.get_by_label("Split This Track Here").click();
    h.run_steps(3);
    let v = h.state().video.as_ref().unwrap();
    assert!(v.playhead.abs_diff(v.session.timeline.length() / 2) < 200_000, "{}", v.playhead);
    assert_eq!(v.session.timeline.tracks.first().map(|t| t.clips.len()), Some(2));
    assert_eq!(v.session.timeline.tracks.get(1).map(|t| t.clips.len()), Some(1), "the others aren't split");
    assert_eq!(v.session.undo_label(), Some("Split"));
}

/// The right button pressed and let go at a point.
fn right_click(h: &mut Harness<'_, Editor>, p: egui::Pos2) {
    h.hover_at(p);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed, modifiers: egui::Modifiers::NONE });
    }
    h.run_steps(3);
}

#[test]
fn the_right_button_on_a_track_takes_the_chosen_stretch_out_of_that_track_alone() {
    let mut h = harness();
    h.state_mut().video.as_mut().unwrap().range = Some((1_000_000, 2_000_000));
    h.run_steps(2);
    let (lane, top) = h.state().video.as_ref().unwrap().lane_rect.unwrap();
    // The second row: the microphone.
    right_click(&mut h, egui::pos2(lane.min.x + 40.0, top + 48.0 + 20.0));
    h.get_by_label_contains("Microphone at");
    h.get_by_label("Remove the Selection from This Track").click();
    h.run_steps(3);
    let v = h.state().video.as_ref().unwrap();
    assert_eq!(v.session.timeline.source_time(1, 1_500_000), None, "silence there");
    assert_eq!(v.session.timeline.source_time(1, 2_500_000), Some(2_500_000), "and after it, still in time");
    assert_eq!(v.session.timeline.source_time(0, 1_500_000), Some(1_500_000), "the screen plays on");
    assert_eq!(length(&h), 4_000_000);
    assert_eq!(v.range, Some((1_000_000, 2_000_000)), "the stretch stays chosen");
}

#[test]
fn the_right_button_on_the_ruler_is_about_every_track_chooses_and_cuts() {
    let mut h = harness();
    let (lane, _) = h.state().video.as_ref().unwrap().lane_rect.unwrap();
    // On the ruler, three quarters of the way along.
    let x = lane.min.x + (lane.width() - 24.0) * 0.75;
    right_click(&mut h, egui::pos2(x, lane.min.y + 8.0));
    h.get_by_label_contains("Every track at");
    assert!(h.query_by_label("Split This Track Here").is_none(), "nothing about one track");
    assert!(egui_kittest::kittest::NodeT::accesskit_node(&h.get_by_label_contains("Cut the Selection")).is_disabled());
    shot(&mut h, "video-ruler-menu");
    h.get_by_label("Select from Here to the End").click();
    h.run_steps(3);
    let (a, b) = h.state().video.as_ref().unwrap().range.expect("a stretch chosen");
    assert!(a.abs_diff(3_000_000) < 150_000 && b == 4_000_000, "{a}..{b}");
    // Again, and Cut: the stretch goes from every track.
    right_click(&mut h, egui::pos2(x, lane.min.y + 8.0));
    h.get_by_label_contains("Cut the Selection").click();
    h.run_steps(3);
    assert_eq!(length(&h), a);
    let v = h.state().video.as_ref().unwrap();
    assert!(v.range.is_none());
    assert!(v.session.timeline.tracks.iter().all(|t| t.clips.last().is_some_and(|c| c.end() == a)));
    // Select All, from the keys; Esc lets it go.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    assert_eq!(h.state().video.as_ref().unwrap().range, Some((0, a)));
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(h.state().video.as_ref().unwrap().range.is_none());
}

#[test]
fn a_handle_beside_the_playhead_drags_out_a_stretch_that_play_keeps_to() {
    let mut h = harness();
    h.state_mut().video.as_mut().unwrap().playhead = 1_000_000;
    h.run_steps(3);
    let (lane, top) = h.state().video.as_ref().unwrap().lane_rect.unwrap();
    let pps = h.state().video.as_ref().unwrap().pps;
    let x_of = |us: u64| lane.min.x + us as f32 / 1e6 * pps;
    // Low on the ruler, where the handles are (the ruler is above the band over the rows).
    let ruler_y = lane.min.y + 20.0;
    assert!(ruler_y < top - 22.0, "{ruler_y} {top}");
    // The handle just right of the playhead's head, dragged to 2.5 s: the stretch is 1 s to 2.5 s,
    // and the picture shows its end meanwhile.
    let handle = egui::pos2(x_of(1_000_000) + 7.0 + 5.5, ruler_y);
    crate::tests::drag(&mut h, handle, egui::pos2(x_of(2_500_000) + 5.5, ruler_y));
    h.run_steps(2);
    let v = h.state().video.as_ref().unwrap();
    let (a, b) = v.range.expect("a stretch chosen by the handle");
    assert_eq!(a, 1_000_000, "the playhead's end stays");
    assert!(b.abs_diff(2_500_000) < 100_000, "{b}");
    assert_eq!(v.playhead, b, "the playhead follows the handle being dragged");
    shot(&mut h, "video-selection-handles");
    // The playhead outside it: Play starts at its start, and stops at its end.
    h.state_mut().video.as_mut().unwrap().playhead = 3_500_000;
    h.key_press(egui::Key::Space);
    h.run_steps(1);
    let v = h.state().video.as_ref().unwrap();
    assert!(v.is_playing());
    assert!(v.playhead >= a && v.playhead < a + 400_000, "from the stretch's start: {}", v.playhead);
    h.run_steps(12);
    let v = h.state().video.as_ref().unwrap();
    assert!(!v.is_playing(), "stopped at the stretch's end");
    assert_eq!(v.playhead, b);
    // The playhead in it: Play starts there.
    let middle = (a + b) / 2;
    h.state_mut().video.as_mut().unwrap().playhead = middle;
    h.key_press(egui::Key::Space);
    h.run_steps(1);
    let v = h.state().video.as_ref().unwrap();
    assert!(v.is_playing() && v.playhead >= middle && v.playhead < middle + 400_000, "{}", v.playhead);
    h.key_press(egui::Key::Space);
    h.run_steps(1);
    // A click on the ruler in the stretch moves the playhead and keeps the stretch; outside it,
    // the stretch is let go and the handles ride with the playhead again.
    crate::tests::click_at(&mut h, egui::pos2(x_of(1_500_000), lane.min.y + 8.0));
    let v = h.state().video.as_ref().unwrap();
    assert!(v.range.is_some() && v.playhead.abs_diff(1_500_000) < 100_000, "{:?} {}", v.range, v.playhead);
    crate::tests::click_at(&mut h, egui::pos2(x_of(3_500_000), lane.min.y + 8.0));
    let v = h.state().video.as_ref().unwrap();
    assert!(v.range.is_none() && v.playhead.abs_diff(3_500_000) < 100_000, "{:?} {}", v.range, v.playhead);
    // Nothing chosen: Play plays on to the end.
    h.state_mut().video.as_mut().unwrap().playhead = 3_000_000;
    h.key_press(egui::Key::Space);
    h.run_steps(8);
    assert_eq!(h.state().video.as_ref().unwrap().playhead, 4_000_000);
}

#[test]
fn a_chosen_clip_is_named_in_the_panel_with_what_is_done_to_it() {
    let mut h = harness();
    assert!(h.query_by_label("Video").is_some(), "nothing chosen: the video's own panel");
    h.state_mut().video.as_mut().unwrap().selected = Some((0, 0));
    h.run_steps(3);
    assert!(h.query_by_label("Screen clip").is_some());
    assert!(h.query_by_label_contains("Starts at 0:00.0").is_some());
    // The playhead is at its very start: nothing to split there.
    assert!(egui_kittest::kittest::NodeT::accesskit_node(&h.get_by_label("Split at Playhead")).is_disabled());
    let half = length(&h) / 2;
    h.state_mut().video.as_mut().unwrap().playhead = half;
    h.run_steps(3);
    shot(&mut h, "video-clip-panel");
    h.get_by_label("Split at Playhead").click();
    h.run_steps(3);
    let v = h.state().video.as_ref().unwrap();
    assert_eq!(v.session.timeline.tracks.first().map(|t| t.clips.len()), Some(2));
    assert_eq!(v.session.timeline.tracks.get(1).map(|t| t.clips.len()), Some(1), "its own track only");
    // Delete Clip takes the chosen one out.
    h.state_mut().video.as_mut().unwrap().selected = Some((0, 1));
    h.run_steps(3);
    h.get_by_label("Delete Clip").click();
    h.run_steps(3);
    let v = h.state().video.as_ref().unwrap();
    assert_eq!(v.session.timeline.tracks.first().map(|t| t.clips.len()), Some(1));
    assert!(v.selected.is_none());
    assert!(h.query_by_label("Video").is_some(), "back to the video's own panel");
    // A stretch chosen across the tracks: said, and let go of.
    h.state_mut().video.as_mut().unwrap().range = Some((500_000, 1_500_000));
    h.run_steps(3);
    assert!(h.query_by_label("Chosen stretch").is_some());
    assert!(h.query_by_label_contains("0:01.0 long").is_some());
    shot(&mut h, "video-stretch-panel");
    h.get_by_label("Let Go").click();
    h.run_steps(3);
    assert!(h.state().video.as_ref().unwrap().range.is_none());
}

#[test]
fn a_chosen_stretch_is_sped_up_from_the_panel_and_a_clip_slowed_down_from_the_menu() {
    let mut h = harness();
    // 1 s to 3 s chosen, made twice as fast: the video is a second shorter, the stretch stays
    // chosen as long as it now is, every track in step.
    h.state_mut().video.as_mut().unwrap().range = Some((1_000_000, 3_000_000));
    h.run_steps(3);
    h.get_by_label("2×").click();
    h.run_steps(3);
    let v = h.state().video.as_ref().unwrap();
    assert_eq!(length(&h), 3_000_000);
    assert_eq!(v.range, Some((1_000_000, 2_000_000)));
    for track in 0..3 {
        assert_eq!(v.session.timeline.source_time(track, 1_500_000), Some(2_000_000), "track {track}");
    }
    assert_eq!(v.session.undo_label(), Some("Speed"));
    // The picture follows: at 1.5 s the box is where it was at 2 s of the recording (x = 120).
    let mut c = screengrabcraft_media::compose::Composer::new(v.rec.clone(), &v.session.timeline, 320, 180);
    let pic = c.frame(&v.session.timeline, 1_500_000);
    assert!(pic.pixel(165, 60).is_some_and(|p| p.b > 150 && p.r < 120), "{:?}", pic.pixel(165, 60));
    // And the sound: it's heard, faster, not silent.
    let sound = screengrabcraft_media::compose::mix(&v.rec, &v.session.timeline, 60_000, 9600);
    assert!(sound.iter().any(|s| s.abs() > 0.1));
    let fast = v.session.timeline.tracks[0].clips.iter().position(|c| c.speed == 200).unwrap();
    shot(&mut h, "video-speed-stretch");
    // Its clip on the screen track, chosen with the right button (at 1.5 s, the timeline still
    // fitted to 4 s): the panel says how fast it plays.
    h.state_mut().video.as_mut().unwrap().range = None;
    h.run_steps(2);
    let (lane, top) = h.state().video.as_ref().unwrap().lane_rect.unwrap();
    right_click(&mut h, egui::pos2(lane.min.x + (lane.width() - 24.0) * 0.375, top + 20.0));
    assert_eq!(h.state().video.as_ref().unwrap().selected, Some((0, fast)));
    assert!(h.query_by_label_contains("playing at 2×").is_some());
    // Speed of the Clip > 0.5×: that second takes four now.
    h.get_by_label_contains("Speed of the Clip").hover();
    h.run_steps(2);
    h.get_by_label("Play at 0.5×").click();
    h.run_steps(3);
    assert_eq!(length(&h), 6_000_000);
    shot(&mut h, "video-speed-clip");
    // Undone twice: as recorded.
    for _ in 0..2 {
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
        h.run_steps(2);
    }
    assert_eq!(length(&h), 4_000_000);
    assert!(h.state().video.as_ref().unwrap().session.timeline.tracks.iter().flat_map(|t| &t.clips).all(|c| c.speed == 100));
}

#[test]
fn a_sound_clip_fades_and_its_tracks_steady_hum_is_taken_out() {
    let mut h = harness();
    // The screen's clip has no sound to fade; the microphone's has, and its track's noise.
    h.state_mut().video.as_mut().unwrap().selected = Some((0, 0));
    h.run_steps(3);
    assert!(h.query_by_label("Fade in").is_none());
    h.state_mut().video.as_mut().unwrap().selected = Some((1, 0));
    h.run_steps(3);
    for label in ["Fade in", "Fade out", "Background noise taken out"] {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
    // A second's fade in and half a second's fade out (as the sliders set them): silent at the
    // start, drawn on the clip.
    let v = h.state_mut().video.as_mut().unwrap();
    v.session.execute("video.fade", &serde_json::json!({"track": 1, "clip": 0, "in": 1_000_000, "out": 500_000})).unwrap();
    let start = screengrabcraft_media::compose::mix(&v.rec, &v.session.timeline, 0, 480);
    assert!(start.iter().all(|s| s.abs() < 0.01));
    h.run_steps(3);
    shot(&mut h, "video-sound-fades");
    // The microphone's row, right button: Take Out Background Noise. Its hum never changes, so
    // it's learnt as the noise and goes.
    let loud = |h: &Harness<'_, Editor>| {
        let v = h.state().video.as_ref().unwrap();
        let s = screengrabcraft_media::compose::mix(&v.rec, &v.session.timeline, 96_000, 9600);
        (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
    };
    let before = loud(&h);
    let (lane, top) = h.state().video.as_ref().unwrap().lane_rect.unwrap();
    right_click(&mut h, egui::pos2(lane.min.x + 40.0, top + 48.0 + 20.0));
    h.get_by_label("Take Out Background Noise").click();
    h.run_steps(3);
    assert_eq!(h.state().video.as_ref().unwrap().session.timeline.tracks[1].denoise, 0.6);
    let after = loud(&h);
    assert!(before > 0.05 && after < before / 2.0, "{before} then {after}");
}

#[test]
fn a_caption_is_put_right_split_joined_and_moved_from_the_transcript() {
    let mut bytes = meeting();
    bytes.extend(screengrabcraft_media::recording::captions_bytes(&captions()).unwrap());
    let mut h = caption_harness_stepping(bytes, CaptionHost::default(), 0.02);
    h.state_mut().video.as_mut().unwrap().transcript_open = true;
    h.run_steps(3);
    let corrected = |h: &Harness<'_, Editor>| {
        h.state()
            .video
            .as_ref()
            .unwrap()
            .session
            .timeline
            .corrected
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|c| (c.text, c.end))
            .collect::<Vec<_>>()
    };
    // Your line, double-clicked: its words in a box, chosen, typed over, kept with Enter.
    let at = h.get_by_label_contains("Here's the new login page.").rect().center();
    h.hover_at(at);
    h.run_steps(1);
    for pressed in [true, false, true, false] {
        h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
    }
    h.run_steps(3);
    assert!(h.state().video.as_ref().unwrap().caption_edit.is_some(), "the double click puts its words in the box");
    h.event(egui::Event::Text("Here's the new sign-in page, with its button on the right.".into()));
    h.run_steps(1);
    shot(&mut h, "video-caption-editing");
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    {
        let v = h.state().video.as_ref().unwrap();
        assert!(v.caption_edit.is_none());
        assert_eq!(v.session.undo_label(), Some("Edit Caption"));
        assert!(v.cues.iter().any(|c| c.text.starts_with("Here's the new sign-in page")), "on the picture and in the video too");
    }
    // Its right button: Split in Two, then joined back.
    let at = h.get_by_label_contains("sign-in page").rect().center();
    right_click(&mut h, at);
    h.get_by_label("Split in Two").click();
    h.run_steps(3);
    assert_eq!(
        corrected(&h).iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>()[..2],
        ["Here's the new sign-in page,", "with its button on the right."]
    );
    let at = h.get_by_label_contains("sign-in page,").rect().center();
    right_click(&mut h, at);
    h.get_by_label("Join with the Next Line").click();
    h.run_steps(3);
    assert_eq!(corrected(&h)[0].0, "Here's the new sign-in page, with its button on the right.");
    // The others' line ends at the playhead (3 s).
    h.state_mut().video.as_mut().unwrap().playhead = 3_000_000;
    h.run_steps(2);
    let at = h.get_by_label_contains("It looks good to me.").rect().center();
    right_click(&mut h, at);
    h.get_by_label("End at the Playhead").click();
    h.run_steps(3);
    assert!(corrected(&h).contains(&("It looks good to me.".to_string(), 3_000_000)));
    shot(&mut h, "video-caption-moved");
    // All as heard again, from the same menu.
    let at = h.get_by_label_contains("It looks good to me.").rect().center();
    right_click(&mut h, at);
    h.get_by_label("All Captions as Heard").click();
    h.run_steps(3);
    assert!(h.state().video.as_ref().unwrap().session.timeline.corrected.is_none());
    assert!(h.query_by_label_contains("Here's the new login page.").is_some());
}

#[test]
fn markers_are_added_with_m_named_in_the_panel_and_deleted() {
    let mut h = harness();
    h.state_mut().video.as_mut().unwrap().playhead = 1_000_000;
    h.run_steps(2);
    h.key_press(egui::Key::M);
    h.run_steps(2);
    h.state_mut().video.as_mut().unwrap().playhead = 3_000_000;
    h.run_steps(2);
    h.get_by_label_contains("Add a Marker at the Playhead").click();
    h.run_steps(3);
    let markers = |h: &Harness<'_, Editor>| {
        h.state().video.as_ref().unwrap().session.timeline.markers.iter().map(|m| (m.at, m.name.clone())).collect::<Vec<_>>()
    };
    assert_eq!(markers(&h), vec![(1_000_000, "Chapter 1".to_string()), (3_000_000, "Chapter 2".to_string())]);
    // The same moment again: said, not added.
    h.key_press(egui::Key::M);
    h.run_steps(2);
    assert_eq!(markers(&h).len(), 2);
    // Named in the panel: typed, kept when the box is left.
    let name = h.query_all_by_value("Chapter 1").next().unwrap().rect().center();
    h.drag_at(name);
    h.drop_at(name);
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.event(egui::Event::Text("The login page".into()));
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(markers(&h)[0].1, "The login page");
    assert_eq!(h.state().video.as_ref().unwrap().session.timeline.chapters()[1], (1_000_000, "The login page".to_string()));
    shot(&mut h, "video-markers");
    // Its time goes there; its button deletes it.
    h.get_by_label("0:03.0").click();
    h.run_steps(2);
    assert_eq!(h.state().video.as_ref().unwrap().playhead, 3_000_000);
    h.get_by_label("Delete the Marker Chapter 2").click();
    h.run_steps(3);
    assert_eq!(markers(&h).len(), 1);
}

#[test]
fn add_text_puts_words_at_the_playhead_ready_to_be_typed_over() {
    let mut h = harness();
    let half = length(&h) / 2;
    h.state_mut().video.as_mut().unwrap().playhead = half;
    h.run_steps(2);
    h.get_by_label("Add Text").click();
    h.run_steps(4);
    let v = h.state().video.as_ref().unwrap();
    let (layer, id) = v.item.expect("the words are chosen");
    let item = v.session.timeline.item(layer, id).cloned().unwrap();
    assert!(item.at.abs_diff(half) < 100_000, "at the playhead: {}", item.at);
    assert!(matches!(&item.what, screengrabcraft_media::layers::What::Text { title, .. } if title == "Your title"));
    // Typing replaces the words it came with: their box has the keys, its words chosen.
    h.event(egui::Event::Text("Step one".into()));
    h.run_steps(4);
    let v = h.state().video.as_ref().unwrap();
    let item = v.session.timeline.item(layer, id).cloned().unwrap();
    assert!(matches!(&item.what, screengrabcraft_media::layers::What::Text { title, .. } if title == "Step one"), "{:?}", item.what);
    shot(&mut h, "video-add-text");
    // T does the same once nothing is being typed (Esc leaves the box), and is one undo step.
    let items = |h: &Harness<'_, Editor>| h.state().video.as_ref().unwrap().session.timeline.layers.iter().map(|l| l.items.len()).sum::<usize>();
    h.key_press(egui::Key::T);
    h.run_steps(3);
    assert_eq!(items(&h), 1, "typed into the box, not taken as the key");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    h.key_press(egui::Key::T);
    h.run_steps(4);
    assert_eq!(items(&h), 2);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(3);
    assert_eq!(items(&h), 1);
}

#[test]
fn the_library_is_searched_by_name_whatever_the_section() {
    let mut h = harness();
    h.state_mut().video.as_mut().unwrap().lib_search = "chapter".into();
    h.run_steps(3);
    assert!(h.query_by_label("Chapter card").is_some(), "found though its section is shut");
    assert!(h.query_by_label("Breaking news").is_none());
    // A section's name finds all of it.
    h.state_mut().video.as_mut().unwrap().lib_search = "training".into();
    h.run_steps(3);
    for label in ["Chapter card", "Next step", "Progress bar", "Done"] {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
    shot(&mut h, "video-library-search");
    h.state_mut().video.as_mut().unwrap().lib_search = "zebra".into();
    h.run_steps(3);
    assert!(h.query_by_label_contains("Nothing in the library").is_some());
    // Cleared, the sections are back.
    h.state_mut().video.as_mut().unwrap().lib_search.clear();
    h.run_steps(3);
    assert!(h.query_by_label("Lower Thirds").is_some() && h.query_by_label("Breaking news").is_some());
}

#[test]
fn a_track_is_hidden_and_shown_by_the_switch_in_its_header() {
    let mut h = harness();
    let muted = |h: &Harness<'_, Editor>| h.state().video.as_ref().unwrap().session.timeline.tracks.first().map(|t| t.muted);
    assert_eq!(muted(&h), Some(false));
    h.query_all_by_label("Show").next().unwrap().click();
    h.run_steps(2);
    assert_eq!(muted(&h), Some(true));
    h.query_all_by_label("Show").next().unwrap().click();
    h.run_steps(2);
    assert_eq!(muted(&h), Some(false));
}

#[test]
fn help_lists_the_video_editors_keys() {
    let mut h = harness();
    h.get_by_label("Help").click();
    h.run_steps(2);
    h.get_by_label("Keyboard Shortcuts").click();
    h.run_steps(10);
    h.get_by_label("Play or pause");
    h.get_by_label_contains("Split at the playhead");
    shot(&mut h, "video-shortcuts");
    h.get_by_label("Close").click();
    h.run_steps(2);
    assert!(h.state().dialog.is_none());
}

#[test]
fn a_wide_window_names_the_video_tool_bars_buttons_as_the_picture_editors_are() {
    let wide = |width: f32| {
        let mut ed = Editor::new(Box::new(NoServices), EditorSettings::default());
        ed.open_file(&egui::Context::default(), OpenedFile { name: "Production.sgvideo".into(), path: None, bytes: recording() });
        let mut h = Harness::builder().with_size(egui::vec2(width, 900.0)).build_ui_state(
            |ui, ed: &mut Editor| {
                let ctx = ui.ctx().clone();
                ed.logic(&ctx);
                ed.ui(ui);
            },
            ed,
        );
        h.run_steps(3);
        h
    };
    let side = crate::editor::BAR_BUTTON;
    // Wide enough for the picture editor's names (the window the editor opens in is): each
    // button has its own under it.
    let mut h = wide(1320.0);
    for label in ["Library", "Open (Ctrl+O)", "Add Text"] {
        // (The first so named: the library's panel has a Library tab of its own.)
        let r = h.query_all_by_label(label).next().unwrap().rect();
        assert!((r.height() - (side + 14.0)).abs() < 0.5, "{label}: {r:?}");
    }
    shot(&mut h, "video-tool-bar-named");
    // They still do what they did.
    h.query_all_by_label("Library").next().unwrap().click();
    h.run_steps(2);
    assert!(!h.state().video.as_ref().unwrap().library_open);
    // Turned off in View, or in a window too narrow for the picture editor's: the bar is as tall
    // as its buttons.
    h.state_mut().settings.hide_tool_names = true;
    h.run_steps(2);
    assert!((h.get_by_label("Open (Ctrl+O)").rect().height() - side).abs() < 0.5);
    let h = wide(800.0);
    assert!((h.get_by_label("Open (Ctrl+O)").rect().height() - side).abs() < 0.5);
}

#[test]
fn a_zoomed_timeline_has_a_bar_to_scroll_by_and_the_ruler_says_the_time_under_the_pointer() {
    let mut h = harness();
    assert!(h.query_by_label("Scroll the Timeline").is_none(), "all of it fits: nothing to scroll");
    for _ in 0..5 {
        h.get_by_label("Zoom the Timeline In (Ctrl++)").click();
        h.run_steps(3);
    }
    let bar = h.get_by_label("Scroll the Timeline").rect();
    assert_eq!(h.state().video.as_ref().unwrap().scrolled(), 0.0);
    crate::tests::drag(&mut h, egui::pos2(bar.min.x + 20.0, bar.center().y), egui::pos2(bar.min.x + 220.0, bar.center().y));
    let along = h.state().video.as_ref().unwrap().scrolled();
    assert!(along > 100.0, "{along}");
    // A click further along its track goes there.
    crate::tests::click_at(&mut h, egui::pos2(bar.max.x - 10.0, bar.center().y));
    assert!(h.state().video.as_ref().unwrap().scrolled() > along);
    // Fitted again, there's nothing to scroll and nothing scrolled.
    h.get_by_label("Fit the Whole Video on the Timeline (Ctrl+0)").click();
    h.run_steps(3);
    assert!(h.query_by_label("Scroll the Timeline").is_none());
    assert_eq!(h.state().video.as_ref().unwrap().scrolled(), 0.0);
    // The pointer over the ruler: the picture shows its time there (looked at, not asserted).
    let (lane, _) = h.state().video.as_ref().unwrap().lane_rect.unwrap();
    h.hover_at(egui::pos2(lane.min.x + lane.width() * 0.6, lane.min.y + 10.0));
    h.run_steps(2);
    shot(&mut h, "video-ruler-time");
}

#[test]
fn an_item_dragged_along_the_timeline_catches_on_the_playhead_and_alt_lets_it_past() {
    let mut h = media_harness(MediaHost::default());
    let (layer, item) = {
        let ctx = egui::Context::default();
        let ed = h.state_mut();
        let mut v = ed.video.take().unwrap();
        v.drop_on_timeline(ed, &ctx, &crate::video::LibDrop::Template("title"), None, 0);
        // The playhead well along, where nothing else ends.
        v.playhead = 2_300_000;
        v.pps = 0.0;
        let chosen = v.item.unwrap();
        ed.video = Some(v);
        chosen
    };
    h.run_steps(3);
    let at = |h: &Harness<'_, Editor>| h.state().video.as_ref().unwrap().session.timeline.item(layer, item).unwrap().at;
    let rect = |h: &Harness<'_, Editor>| h.state().video.as_ref().unwrap().item_on_timeline(layer, item).unwrap();
    let pps = h.state().video.as_ref().unwrap().pps;
    // Dragged so its start is a few points short of the playhead: it's caught there, exactly.
    let r = rect(&h);
    let nearly = 2_300_000.0 / 1e6 * pps - 4.0;
    crate::tests::drag(&mut h, r.center(), r.center() + egui::vec2(nearly, 0.0));
    assert_eq!(at(&h), 2_300_000, "its start is on the playhead");
    // Back where it was, then the same with Alt held: it stays where it was put.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(2);
    assert_eq!(at(&h), 0);
    let r = rect(&h);
    let (from, to) = (r.center(), r.center() + egui::vec2(nearly, 0.0));
    let alt = egui::Modifiers::ALT;
    h.hover_at(from);
    h.run_steps(1);
    h.event(egui::Event::ModifiersChanged(alt));
    h.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: alt });
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(from + (to - from) * (k as f32 / 4.0));
        h.run_steps(1);
    }
    h.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: alt });
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run_steps(3);
    let free = at(&h);
    assert!(free != 2_300_000 && free.abs_diff(2_300_000) < 200_000, "near it, not on it: {free}");
}

#[test]
fn an_item_dragged_near_the_middle_of_the_picture_is_caught_there() {
    let mut h = media_harness(MediaHost::default());
    {
        let ctx = egui::Context::default();
        let ed = h.state_mut();
        let mut v = ed.video.take().unwrap();
        v.playhead = 2_000_000;
        v.drop_on_timeline(ed, &ctx, &crate::video::LibDrop::Template("news-name"), None, 0);
        ed.video = Some(v);
    }
    h.run_steps(3);
    let (pic, on) = {
        let v = h.state().video.as_ref().unwrap();
        (v.picture_rect.unwrap(), v.chosen_on_screen().unwrap())
    };
    // Let go a few points off the middle, both ways: its middle is on the picture's (to a pixel
    // of this small video, which is two points here).
    crate::tests::drag(&mut h, on.center(), pic.center() + egui::vec2(5.5, -5.5));
    let now = h.state().video.as_ref().unwrap().chosen_on_screen().unwrap();
    assert!((now.center().x - pic.center().x).abs() < 2.0 && (now.center().y - pic.center().y).abs() < 2.0, "{now:?} in {pic:?}");
    // Well away from the middle and the edges, it stays where it was put.
    crate::tests::drag(&mut h, now.center(), pic.center() + egui::vec2(60.0, 40.0));
    let away = h.state().video.as_ref().unwrap().chosen_on_screen().unwrap();
    assert!((away.center().x - pic.center().x - 60.0).abs() < 6.0 && (away.center().y - pic.center().y - 40.0).abs() < 6.0, "{away:?}");
}

#[test]
fn the_video_editor_is_readable_in_the_light_look() {
    let settings = EditorSettings { theme: screengrabcraft_model::Theme::Light, ..EditorSettings::default() };
    let mut ed = Editor::new(Box::new(MediaHost::default()), settings);
    let path = Some("C:/Captures/Production.sgvideo".to_string());
    ed.open_file(&egui::Context::default(), OpenedFile { name: "Production.sgvideo".into(), path, bytes: recording() });
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 900.0)).build_ui_state(
        |ui, ed: &mut Editor| {
            let ctx = ui.ctx().clone();
            ed.logic(&ctx);
            ed.ui(ui);
        },
        ed,
    );
    h.run_steps(3);
    {
        let ctx = egui::Context::default();
        let ed = h.state_mut();
        let mut v = ed.video.take().unwrap();
        v.playhead = 2_000_000;
        v.drop_on_timeline(ed, &ctx, &crate::video::LibDrop::Template("news-name"), None, 0);
        ed.video = Some(v);
    }
    // A track switched off, to see its switch.
    h.run_steps(3);
    h.query_all_by_label("Show").nth(2).unwrap().click();
    h.run_steps(3);
    assert!(!crate::theme::Tokens::get(&h.ctx).dark, "the light look is the one drawn");
    for label in ["Text", "Look", "Position", "Animation", "Entrance", "Exit"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    shot(&mut h, "video-editor-light");
}

#[test]
fn the_right_button_on_nothing_lets_go_of_the_clip_chosen_and_splits_every_track() {
    let mut h = harness();
    let (lane, top) = h.state().video.as_ref().unwrap().lane_rect.unwrap();
    // A clip chosen first, on the first track.
    crate::tests::click_at(&mut h, egui::pos2(lane.min.x + 60.0, top + 20.0));
    assert_eq!(h.state().video.as_ref().unwrap().selected, Some((0, 0)));
    // Then the right button on the band above the rows, where there is nothing.
    let p = egui::pos2(lane.min.x + (lane.width() - 24.0) / 2.0, top - 10.0);
    h.hover_at(p);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed, modifiers: egui::Modifiers::NONE });
    }
    h.run_steps(3);
    assert_eq!(h.state().video.as_ref().unwrap().selected, None);
    h.get_by_label_contains("Every track at");
    h.get_by_label_contains("Split Every Track Here").click();
    h.run_steps(3);
    let v = h.state().video.as_ref().unwrap();
    assert!(v.session.timeline.tracks.iter().all(|t| t.clips.len() == 2), "every track is split: {:?}", v.session.timeline.tracks);
}

#[test]
fn every_library_card_is_drawn_and_shows_its_own_item() {
    let lib = screengrabcraft_media::layers::library();
    let cards: Vec<(&str, Raster)> = lib
        .iter()
        .map(|t| (t.key, crate::video::VideoView::template_card_picture(t.key).unwrap_or_else(|| panic!("{} has no card", t.key))))
        .collect();
    for (i, (a, pa)) in cards.iter().enumerate() {
        for (b, pb) in cards.iter().skip(i + 1) {
            assert_ne!(pa.pixels(), pb.pixels(), "the cards of {a} and {b} are the same picture");
        }
    }
    // A sheet of them all, to look at.
    if let Ok(dir) = std::env::var("SGC_SHOTS") {
        let (w, h) = (cards[0].1.width(), cards[0].1.height());
        let mut sheet = image::RgbaImage::new(w * 6 + 50, h * cards.len().div_ceil(6) as u32 + 10 * cards.len().div_ceil(6) as u32);
        for (k, (_, pic)) in cards.iter().enumerate() {
            let card = image::RgbaImage::from_raw(pic.width(), pic.height(), pic.pixels().to_vec()).unwrap();
            let (x, y) = ((k as u32 % 6) * (w + 10), (k as u32 / 6) * (h + 10));
            image::imageops::overlay(&mut sheet, &card, i64::from(x), i64::from(y));
        }
        sheet.save(format!("{dir}/video-library-cards.png")).unwrap();
    }
}

/// A host with both editors' windows, as the desktop's: files of the other kind are handed to
/// the other window (and noted here).
#[derive(Clone, Default)]
struct TwoWindows(std::sync::Arc<std::sync::Mutex<Vec<(String, bool)>>>);

impl crate::services::Services for TwoWindows {
    fn open_in_other_editor(&self, path: &str, video: bool) -> bool {
        self.0.lock().unwrap().push((path.to_string(), video));
        true
    }

    fn can_capture(&self) -> bool {
        true
    }

    fn recent(&self) -> Vec<crate::services::RecentItem> {
        let item = |name: &str| crate::services::RecentItem {
            path: format!("C:/Captures/{name}"),
            name: name.to_string(),
            modified: 1,
            day: String::new(),
            time: String::new(),
        };
        vec![item("Week.sgvideo"), item("Shot.png"), item("Week.mp4"), item("Notes.sgcraft"), item("Loop.gif")]
    }
}

fn two_windows(kind: crate::EditorKind) -> (Harness<'static, Editor>, TwoWindows) {
    let host = TwoWindows::default();
    let mut ed = Editor::new(Box::new(host.clone()), EditorSettings { show_tray: true, ..EditorSettings::default() });
    ed.kind = kind;
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_step_dt(0.25).build_ui_state(
        |ui, ed: &mut Editor| {
            let ctx = ui.ctx().clone();
            ed.logic(&ctx);
            ed.ui(ui);
        },
        ed,
    );
    h.run_steps(3);
    (h, host)
}

#[test]
fn the_picture_editor_hands_a_recording_to_the_video_editor_and_keeps_its_picture() {
    let (mut h, host) = two_windows(crate::EditorKind::Pictures);
    let before = h.state().session.doc.base.pixels().to_vec();
    let ctx = h.ctx.clone();
    h.state_mut().open_file(&ctx, OpenedFile { name: "Week.sgvideo".into(), path: Some("C:/Captures/Week.sgvideo".into()), bytes: recording() });
    h.run_steps(2);
    assert_eq!(*host.0.lock().unwrap(), vec![("C:/Captures/Week.sgvideo".to_string(), true)]);
    assert!(h.state().video.is_none(), "the recording isn't opened here");
    assert_eq!(h.state().session.doc.base.pixels(), &before[..], "the picture stays");
    // Without a file to hand over (a paste, a browser), it opens here, as it always did.
    h.state_mut().open_file(&ctx, OpenedFile { name: "Week.sgvideo".into(), path: None, bytes: recording() });
    h.run_steps(2);
    assert!(h.state().video.is_some());
}

#[test]
fn the_video_editor_starts_with_its_own_page_opens_recordings_and_hands_pictures_over() {
    let (mut h, host) = two_windows(crate::EditorKind::Video);
    // Nothing open: its own start, not a picture to draw on.
    for label in ["No recording open", "Record a Video", "Open a Recording…", "Open (Ctrl+O)", "New Recording"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    assert!(h.query_by_label("Arrow").is_none() && h.query_by_label("Capture an Area").is_none(), "no picture tools");
    shot(&mut h, "video-editor-start");
    // A recording opens in it.
    let ctx = h.ctx.clone();
    h.state_mut().open_file(&ctx, OpenedFile { name: "Week.sgvideo".into(), path: None, bytes: recording() });
    h.run_steps(3);
    assert!(h.state().video.is_some());
    let edits = h.state().video.as_ref().unwrap().session.revision();
    // A picture goes to the picture editor, and the video being edited stays.
    let png =
        screengrabcraft_render::codec::encode(&Raster::filled(40, 30, Color::rgb(200, 30, 30)).unwrap(), screengrabcraft_model::ImageFormat::Png, 90)
            .unwrap();
    h.state_mut().open_file(&ctx, OpenedFile { name: "Shot.png".into(), path: Some("C:/Captures/Shot.png".into()), bytes: png });
    h.run_steps(2);
    assert_eq!(*host.0.lock().unwrap(), vec![("C:/Captures/Shot.png".to_string(), false)]);
    let v = h.state().video.as_ref().expect("the video is still open");
    assert_eq!(v.session.revision(), edits);
}

#[test]
fn each_editors_recent_strip_lists_its_own_kind() {
    let has = |h: &Harness<'_, Editor>, name: &str| h.query_all_by_label(name).next().is_some();
    // The Image Editor: pictures and projects, a GIF too; no recordings, no videos.
    let (mut h, _) = two_windows(crate::EditorKind::Pictures);
    h.run_steps(16);
    assert!(has(&h, "Recent captures") && has(&h, "All captures (3)"));
    for name in ["Shot.png", "Notes.sgcraft", "Loop.gif"] {
        assert!(has(&h, name), "{name}");
    }
    for name in ["Week.sgvideo", "Week.mp4"] {
        assert!(!has(&h, name), "{name} isn't the Image Editor's");
    }
    // The Video Editor: its recordings and the videos made of them, nothing else.
    let (mut h, _) = two_windows(crate::EditorKind::Video);
    h.run_steps(16);
    assert!(has(&h, "Recent recordings") && has(&h, "All recordings (2)"));
    for name in ["Week.sgvideo", "Week.mp4"] {
        assert!(has(&h, name), "{name}");
    }
    for name in ["Shot.png", "Notes.sgcraft", "Loop.gif"] {
        assert!(!has(&h, name), "{name} isn't the Video Editor's");
    }
    shot(&mut h, "video-editor-recent");
}

#[test]
fn the_transcript_pane_says_who_said_what_goes_there_and_names_the_speakers() {
    let mut bytes = meeting();
    bytes.extend(screengrabcraft_media::recording::captions_bytes(&captions()).unwrap());
    let mut h = caption_harness(bytes, CaptionHost::default());
    assert!(h.query_by_label("Copy the Whole Transcript").is_none(), "closed at first");
    h.query_all_by_label("Transcript").next().unwrap().click();
    h.run_steps(3);
    assert!(h.state().video.as_ref().unwrap().transcript_open);
    // Each line with when it is and who said it: the microphone's You, the computer's Others.
    h.get_by_label("0:00.2 You: Here's the new login page.");
    h.get_by_label("0:02.0 Others: It looks good to me.").click();
    h.run_steps(2);
    assert_eq!(h.state().video.as_ref().unwrap().playhead, 2_000_000, "a line takes the playhead there");
    shot(&mut h, "video-transcript");
    // You renamed: the microphone's track has the name, in its header and the transcript.
    h.get_by_label("Rename You").click();
    h.run_steps(2);
    h.event(egui::Event::Text("Bill".into()));
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    let v = h.state().video.as_ref().unwrap();
    let mic = v.session.timeline.find(StreamKind::Microphone).unwrap();
    assert_eq!(v.session.timeline.tracks[mic].name.as_deref(), Some("Bill"));
    assert!(v.named_lines().iter().any(|(c, who)| who == "Bill" && c.text.starts_with("Here's")));
    h.get_by_label("0:00.2 Bill: Here's the new login page.");
    assert!(h.query_all_by_label("Bill").next().is_some(), "the track's header says it too");
    // Esc lets a rename go.
    h.get_by_label("Rename Others").click();
    h.run_steps(2);
    h.event(egui::Event::Text("Nobody".into()));
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    let v = h.state().video.as_ref().unwrap();
    let sound = v.session.timeline.find(StreamKind::SystemSound).unwrap();
    assert_eq!(v.session.timeline.tracks[sound].name, None);
    // Closed from its own heading.
    h.get_by_label("Close the Transcript").click();
    h.run_steps(2);
    assert!(!h.state().video.as_ref().unwrap().transcript_open);
}

#[test]
fn tracks_move_up_and_down_and_are_renamed_by_a_double_click_on_their_name() {
    let mut h = harness_stepping(0.02);
    let kinds = |h: &Harness<'_, Editor>| h.state().video.as_ref().unwrap().session.timeline.tracks.iter().map(|t| t.kind).collect::<Vec<_>>();
    assert_eq!(kinds(&h), vec![StreamKind::Screen, StreamKind::Microphone, StreamKind::Camera]);
    assert!(egui_kittest::kittest::NodeT::accesskit_node(&h.get_by_label("Move Screen Up")).is_disabled(), "the top one goes no higher");
    h.get_by_label("Move Camera Up").click();
    h.run_steps(3);
    assert_eq!(kinds(&h), vec![StreamKind::Screen, StreamKind::Camera, StreamKind::Microphone]);
    h.get_by_label("Move Screen Down").click();
    h.run_steps(3);
    assert_eq!(kinds(&h), vec![StreamKind::Camera, StreamKind::Screen, StreamKind::Microphone]);
    // Its name double-clicked: typed over, kept with Enter. (A moment first: the click on Move
    // Down just before would make the two a triple click.)
    h.run_steps(40);
    let name = h.get_by_label("Microphone").rect().center();
    h.hover_at(name);
    h.run_steps(1);
    for _ in 0..2 {
        h.drag_at(name);
        h.drop_at(name);
    }
    h.run_steps(3);
    assert!(h.state().video.as_ref().unwrap().renaming.is_some(), "the double click starts renaming");
    h.event(egui::Event::Text("Narrator".into()));
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    let v = h.state().video.as_ref().unwrap();
    assert_eq!(v.session.timeline.tracks[2].name.as_deref(), Some("Narrator"));
    assert_eq!(v.session.undo_label(), Some("Rename Track"));
    shot(&mut h, "video-tracks-moved");
    // Hidden and shown again from the right button's menu, as from its switch.
    let (lane, top) = h.state().video.as_ref().unwrap().lane_rect.unwrap();
    right_click(&mut h, egui::pos2(lane.min.x + 40.0, top + 20.0));
    h.get_by_label("Hide This Track").click();
    h.run_steps(3);
    assert!(h.state().video.as_ref().unwrap().session.timeline.tracks[0].muted);
}

#[test]
fn layers_move_in_front_and_behind_and_are_renamed() {
    let mut h = harness();
    {
        let v = h.state_mut().video.as_mut().unwrap();
        for (key, at) in [("title", 0), ("lower-bar", 0)] {
            v.session.execute("video.place", &serde_json::json!({"template": key, "at": at, "layer": "new"})).unwrap();
        }
    }
    h.run_steps(3);
    let names =
        |h: &Harness<'_, Editor>| h.state().video.as_ref().unwrap().session.timeline.layers.iter().map(|l| l.name.clone()).collect::<Vec<_>>();
    // The later layer is in front, at the top of the rows.
    assert_eq!(names(&h), vec!["Layer 1", "Layer 2"]);
    assert!(egui_kittest::kittest::NodeT::accesskit_node(&h.get_by_label("Move Layer 2 Up")).is_disabled(), "it's in front already");
    h.get_by_label("Move Layer 1 Up").click();
    h.run_steps(3);
    assert_eq!(names(&h), vec!["Layer 2", "Layer 1"], "Layer 1 in front now");
    // Renamed from the right button's menu.
    let (lane, top) = h.state().video.as_ref().unwrap().lane_rect.unwrap();
    right_click(&mut h, egui::pos2(lane.max.x - 30.0, top + 20.0));
    h.get_by_label("Rename…").click();
    h.run_steps(2);
    h.event(egui::Event::Text("Titles".into()));
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(names(&h), vec!["Layer 2", "Titles"]);
}

#[test]
fn a_zoom_from_the_library_is_placed_on_the_whole_screen_and_zooms_once_let_go() {
    use screengrabcraft_media::layers::What;
    let mut h = media_harness(MediaHost::default());
    // Its section of the library, opened by its heading.
    h.get_by_label("Zoom and Pan").click();
    h.run_steps(3);
    for label in ["Zoom in", "Zoom in close", "Zoom in at once"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    // Its card shows the made-up screen zoomed: closer is another picture.
    let half = crate::video::VideoView::template_card_picture("zoom").unwrap();
    let close = crate::video::VideoView::template_card_picture("zoom-close").unwrap();
    assert_ne!(half.pixels(), close.pixels());
    if let Some(v) = h.state_mut().video.as_mut() {
        v.playhead = 500_000;
    }
    h.run_steps(1);
    h.get_by_label("Add Zoom in").click();
    h.run_steps(3);
    shot(&mut h, "video-zoom-chosen");
    let (layer, item) = {
        let v = h.state().video.as_ref().unwrap();
        let (layer, item) = v.item.expect("the zoom is chosen");
        let it = v.session.timeline.item(layer, item).unwrap();
        assert!(matches!(it.what, What::Zoom));
        assert_eq!(it.at, 500_000, "at the playhead");
        assert!(!v.zooms_shown(), "while it's chosen the whole screen shows, to place it");
        (layer, item)
    };
    for label in ["Zoom", "Shows of the screen", "Across", "Down", "In", "Out", "Delete"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label}");
    }
    // Closer, by its panel's numbers (as its slider does), one undo step.
    if let Some(v) = h.state_mut().video.as_mut() {
        v.session.execute("video.setItem", &serde_json::json!({"layer": layer, "item": item, "w": 0.25, "x": 0.8, "outro": "none"})).unwrap();
    }
    h.run_steps(2);
    {
        let v = h.state_mut().video.as_mut().unwrap();
        let it = v.session.timeline.item(layer, item).unwrap().clone();
        assert_eq!((it.place.w, it.place.x), (0.25, 0.8));
        assert_eq!(it.outro.kind, screengrabcraft_media::layers::AnimKind::None);
        let view = screengrabcraft_media::zoom::view_at(&v.session.timeline, 2_000_000);
        assert!((view.w - 0.25).abs() < 1e-6 && (view.x - 0.675).abs() < 1e-6, "{view:?}");
        // Let go: the picture zooms again.
        v.item = None;
    }
    h.run_steps(2);
    assert!(h.state().video.as_ref().unwrap().zooms_shown());
}

#[test]
fn a_transition_from_the_library_goes_where_two_clips_meet_and_its_panel_changes_it() {
    use screengrabcraft_media::timeline::Blend;
    let mut h = harness();
    // 1.5 s to 2.5 s taken out: every track's two clips meet at 1.5 s.
    h.state_mut().video.as_mut().unwrap().session.execute("video.cut", &serde_json::json!({"from": 1_500_000, "to": 2_500_000})).unwrap();
    h.run_steps(2);
    h.get_by_label("Between Clips").click();
    h.run_steps(3);
    for kind in Blend::ALL {
        assert!(h.query_all_by_label(kind.label()).next().is_some(), "{kind:?}");
    }
    // + puts it at the cut nearest the playhead.
    h.get_by_label("Add Cross-fade").click();
    h.run_steps(3);
    {
        let v = h.state().video.as_ref().unwrap();
        assert_eq!(v.selected, Some((0, 1)), "the clip it leads into is chosen");
        assert_eq!(v.session.timeline.tracks[0].clips[1].transition.map(|x| (x.kind, x.len)), Some((Blend::CrossFade, 1_000_000)));
    }
    // Its panel makes it longer.
    h.get_by_label("2 s").click();
    h.run_steps(2);
    assert_eq!(h.state().video.as_ref().unwrap().session.timeline.tracks[0].clips[1].transition.map(|x| x.len), Some(2_000_000));
    // At the cut the picture is the two together: the box recorded at 1.5 s half there.
    let pic = h.state_mut().video.as_mut().unwrap().frame_at(1_500_000);
    let p = pic.pixel(100, 60).unwrap();
    assert!(p.r > 0x4E + 30 && p.r < 0xFF - 30, "half the box, half the white: {p:?}");
    if let Some(v) = h.state_mut().video.as_mut() {
        v.playhead = 1_500_000;
    }
    h.run_steps(3);
    shot(&mut h, "video-clip-transition");
    // (The library's cards closed, so the menu's names are the only ones.)
    h.get_by_label("Between Clips").click();
    h.run_steps(2);
    // The right button on the clip: another kind.
    let (lane, rows_top, pps, scroll) = {
        let v = h.state().video.as_ref().unwrap();
        let (lane, top) = v.lane_rect.unwrap();
        (lane, top, v.pps, v.scrolled())
    };
    right_click(&mut h, egui::pos2(lane.min.x + 2.2 * pps - scroll, rows_top + 20.0));
    h.get_by_label_contains("Transition from the Clip Before").hover();
    h.run_steps(2);
    h.get_by_label("Push").click();
    h.run_steps(2);
    assert_eq!(h.state().video.as_ref().unwrap().session.timeline.tracks[0].clips[1].transition.map(|x| x.kind), Some(Blend::Push));
    // Dropped on the microphone's row, it's a cross-fade of its sound.
    {
        let ctx = egui::Context::default();
        let ed = h.state_mut();
        let mut v = ed.video.take().unwrap();
        let mic = v.session.timeline.find(StreamKind::Microphone).unwrap();
        v.drop_blend(ed, &ctx, Blend::WipeUp, Some(mic), 1_400_000);
        assert_eq!(v.session.timeline.tracks[mic].clips[1].transition.map(|x| x.kind), Some(Blend::CrossFade));
        ed.video = Some(v);
    }
}

#[test]
fn every_transition_card_is_a_picture_of_its_own() {
    use screengrabcraft_media::timeline::Blend;
    let cards: Vec<Raster> = Blend::ALL.iter().map(|k| crate::video::VideoView::blend_card_picture(*k).unwrap()).collect();
    for (i, a) in cards.iter().enumerate() {
        for b in cards.iter().skip(i + 1) {
            assert_ne!(a.pixels(), b.pixels());
        }
    }
}

#[test]
fn keyframes_added_from_the_panel_move_a_title_across_the_picture() {
    let mut h = media_harness(MediaHost::default());
    {
        let ctx = egui::Context::default();
        let ed = h.state_mut();
        let mut v = ed.video.take().unwrap();
        v.playhead = 0;
        v.drop_on_timeline(ed, &ctx, &crate::video::LibDrop::Template("title"), None, 0);
        ed.video = Some(v);
    }
    h.run_steps(3);
    let (layer, item) = h.state().video.as_ref().unwrap().item.unwrap();
    let keys = |h: &Harness<'_, Editor>| h.state().video.as_ref().unwrap().session.timeline.item(layer, item).unwrap().keys.clone();
    scroll_side_panel(&mut h, 12);
    h.get_by_label("Add Keyframe at the Playhead").click();
    h.run_steps(2);
    assert_eq!(keys(&h).len(), 1);
    // Later, dragged on the picture: a second keyframe there, the first kept.
    if let Some(v) = h.state_mut().video.as_mut() {
        v.playhead = 3_000_000;
    }
    h.run_steps(3);
    let (pic, on) = {
        let v = h.state().video.as_ref().unwrap();
        (v.picture_rect.unwrap(), v.chosen_on_screen().unwrap())
    };
    crate::tests::drag(&mut h, on.center(), on.center() + egui::vec2(pic.width() * 0.3, 0.0));
    let k = keys(&h);
    assert_eq!(k.len(), 2, "{k:?}");
    assert_eq!(k[1].at, 3_000_000);
    assert!(k[1].place.x > k[0].place.x + 0.2, "{k:?}");
    // Halfway, it's halfway across.
    let it = h.state().video.as_ref().unwrap().session.timeline.item(layer, item).unwrap().clone();
    let mid = it.place_at(1_500_000).x;
    assert!((mid - (k[0].place.x + k[1].place.x) / 2.0).abs() < 0.01);
    assert_eq!(h.query_all_by_label("Go To").count(), 2);
    shot(&mut h, "video-keyframes");
    // One taken away from the panel.
    scroll_side_panel(&mut h, 12);
    h.get_all_by_label("Remove").next().unwrap().click();
    h.run_steps(2);
    assert_eq!(keys(&h).len(), 1);
}

/// Scrolls the panel at the right down by `lines` turns of the wheel.
fn scroll_side_panel(h: &mut Harness<'_, Editor>, lines: usize) {
    let r = h.ctx.content_rect();
    h.hover_at(egui::pos2(r.max.x - 120.0, r.min.y + 300.0));
    h.run_steps(1);
    for _ in 0..lines {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, -3.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(10);
}
