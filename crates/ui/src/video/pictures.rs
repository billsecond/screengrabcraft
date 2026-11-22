//! A video made only of pictures (File › New Video from Pictures): a new, empty recording (a
//! blank screen as long as the pictures take, written as an ordinary `.sgvideo` in the captures
//! folder) opened here, the pictures brought into its Media bin and put one after another on a
//! layer over it. Narration is then recorded over it like over any video.

use serde_json::json;

use super::VideoView;
use crate::editor::Editor;
use crate::services::OpenedFile;

/// The size and frame rate of a video made of pictures.
pub const SIZE: (u32, u32) = (1920, 1080);
pub const FPS: u32 = 30;
/// The most pictures one is made of.
pub const MOST: usize = 500;
/// How long each picture shows at first (µs).
pub const EACH_US: u64 = 4_000_000;

/// Pictures chosen for a new video, waiting to be read into its Media bin before they're put in
/// turn: where each is, and how long each shows.
#[derive(Clone, Debug, PartialEq)]
pub struct PicturesPending {
    pub paths: Vec<String>,
    pub each: u64,
}

/// Makes a new video of the pictures at `paths` and opens it here (the video open before is saved
/// first).
pub fn new_video_from_pictures(ed: &mut Editor, ctx: &egui::Context, paths: &[String]) {
    use screengrabcraft_media::layers::MediaKind;
    let pictures: Vec<String> =
        paths.iter().filter(|p| screengrabcraft_media::media_store::kind_of(p) == Some(MediaKind::Picture)).take(MOST).cloned().collect();
    if pictures.is_empty() {
        return ed.error(ctx, "Choose pictures (PNG, JPEG, GIF, WebP or BMP) to make a video of.".to_string());
    }
    let each = ed.video.as_ref().map_or(EACH_US, |v| (f64::from(v.picture_secs.clamp(1.0, 30.0)) * 1e6) as u64);
    let length = each.saturating_mul(pictures.len() as u64);
    let s = ed.services.now();
    let created = format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", s.year, s.month, s.day, s.hour, s.minute, s.second);
    let made = screengrabcraft_media::recording::blank(SIZE.0, SIZE.1, FPS, length, &created).map_err(|e| e.to_string()).and_then(|bytes| {
        let path = ed.services.new_recording_path("Pictures")?;
        ed.services.write_file(&path, &bytes)?;
        Ok((path, bytes))
    });
    let (path, bytes) = match made {
        Ok(m) => m,
        Err(e) => return ed.error(ctx, format!("The new video couldn't be made: {e}")),
    };
    // The video open before keeps its edits.
    if let Some(mut v) = ed.video.take() {
        v.save_now(ed, ctx);
        ed.video = Some(v);
    }
    let name = crate::editor::file_name(&path);
    ed.open_file(ctx, OpenedFile { name, path: Some(path), bytes });
    if let Some(mut v) = ed.video.take() {
        v.picture_secs = (each as f64 / 1e6) as f32;
        v.import_paths(ed, ctx, &pictures);
        v.pictures_pending = Some(PicturesPending { paths: pictures, each });
        ed.video = Some(v);
    }
}

impl VideoView {
    /// The pictures of a new video, once they're all in its Media bin (or couldn't be read): put
    /// one after another from its start.
    pub(super) fn poll_pictures(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        let Some(p) = &self.pictures_pending else { return };
        if p.paths.iter().any(|path| self.importing.iter().any(|i| &i.path == path)) {
            return;
        }
        let Some(p) = self.pictures_pending.take() else { return };
        let ids: Vec<u32> = p.paths.iter().filter_map(|path| self.session.timeline.media.iter().find(|m| &m.path == path).map(|m| m.id)).collect();
        if ids.is_empty() {
            return;
        }
        match self.session.execute("video.picturesInTurn", &json!({"media": ids, "each": p.each, "at": 0})) {
            Ok(_) => {
                self.playhead = 0;
                ed.toast(ctx, format!("A video of {} pictures: talk over it with Record Narration", ids.len()));
            }
            Err(e) => ed.error(ctx, e.to_string()),
        }
    }
}
