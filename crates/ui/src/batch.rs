//! Batch Changes (File menu, and All captures' Change These): the same changes made to many
//! pictures at once. The pictures are listed, the changes chosen (resize, the effects with their
//! settings as in their own windows, grayscale, invert, trim), the format and a file name pattern
//! given; each picture is then changed by the engine's commands (one a frame, so the window keeps
//! drawing) and saved beside the original under the new name. No picture is written over.

use egui::Ui;
use screengrabcraft_model::{ImageFormat, Stamp};
use serde_json::{Value, json};

use crate::dialogs::{self, Dialog};
use crate::editor::{Editor, file_name};
use crate::theme::Tokens;
use crate::widgets::{self, FooterButton, Role};

/// The most pictures in one batch.
pub const MAX_FILES: usize = 5000;
/// The name pattern a batch starts with.
pub const DEFAULT_PATTERN: &str = "{name} changed";
/// The formats changed pictures can be saved as (a GIF's or a PDF's one picture isn't the point).
const FORMATS: [ImageFormat; 4] = [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Webp, ImageFormat::Bmp];

/// One change made to every picture.
#[derive(Clone, Debug)]
pub enum Change {
    /// Made smaller or larger: by a percentage, or to a width (the height kept in proportion).
    Resize {
        by_percent: bool,
        percent: u32,
        width: u32,
    },
    /// An effect, with its settings as in its own window (border, drop shadow, torn edge, round
    /// corners, brightness and contrast).
    Effect(Dialog),
    Grayscale,
    Invert,
    Trim,
}

impl Change {
    /// Every kind of change, as the Add a Change menu offers them.
    pub fn menu() -> Vec<(&'static str, Change)> {
        vec![
            ("Resize", Change::Resize { by_percent: true, percent: 50, width: 1280 }),
            ("Border", Change::Effect(Dialog::border())),
            ("Drop Shadow", Change::Effect(Dialog::shadow())),
            ("Torn Edge", Change::Effect(Dialog::torn())),
            ("Round Corners", Change::Effect(Dialog::round())),
            ("Brightness and Contrast", Change::Effect(Dialog::brightness())),
            ("Grayscale", Change::Grayscale),
            ("Invert Colors", Change::Invert),
            ("Trim Borders", Change::Trim),
        ]
    }

    pub fn title(&self) -> &'static str {
        match self {
            Change::Resize { .. } => "Resize",
            Change::Effect(d) => d.title(),
            Change::Grayscale => "Grayscale",
            Change::Invert => "Invert Colors",
            Change::Trim => "Trim Borders",
        }
    }

    /// Its settings, and the command it comes to.
    fn ui(&mut self, ui: &mut Ui) -> Option<(&'static str, Value)> {
        match self {
            Change::Resize { by_percent, percent, width } => {
                ui.horizontal(|ui| {
                    ui.radio_value(by_percent, true, "By");
                    ui.add_enabled(*by_percent, egui::DragValue::new(percent).range(1..=400).suffix(" %"));
                    ui.radio_value(by_percent, false, "To a width of");
                    ui.add_enabled(!*by_percent, egui::DragValue::new(width).range(1..=65_535).suffix(" px"));
                });
                self.command()
            }
            Change::Effect(d) => dialogs::body(d, ui),
            _ => self.command(),
        }
    }

    /// The command it comes to.
    fn command(&self) -> Option<(&'static str, Value)> {
        Some(match self {
            Change::Effect(d) => return dialogs::command(d),
            Change::Resize { by_percent: true, percent, .. } => ("image.resize", json!({ "percent": percent })),
            Change::Resize { width, .. } => ("image.resize", json!({ "width": width })),
            Change::Grayscale => ("effect.grayscale", json!({})),
            Change::Invert => ("effect.invert", json!({})),
            Change::Trim => ("image.trim", json!({})),
        })
    }
}

/// What a batch has done so far.
#[derive(Clone, Debug, Default)]
pub struct Progress {
    /// The next picture to change.
    pub next: usize,
    /// The files written.
    pub written: Vec<String>,
    /// The pictures that couldn't be changed, and why.
    pub failed: Vec<(String, String)>,
    pub done: bool,
}

/// The Batch Changes window.
#[derive(Clone, Debug)]
pub struct Batch {
    pub files: Vec<String>,
    /// Each change, and the command it came to when it was last drawn.
    pub changes: Vec<(Change, Option<(&'static str, Value)>)>,
    /// The format the changed pictures are saved as; none: each as it was.
    pub format: Option<ImageFormat>,
    pub pattern: String,
    /// Once Change is pressed.
    pub progress: Option<Progress>,
    /// When it started (for `{date}` and `{time}` in the name).
    stamp: Stamp,
}

impl Batch {
    pub fn new(files: Vec<String>) -> Batch {
        let mut b = Batch {
            files: Vec::new(),
            changes: Vec::new(),
            format: None,
            pattern: DEFAULT_PATTERN.to_string(),
            progress: None,
            stamp: Stamp::default(),
        };
        b.add(files);
        b
    }

    /// Adds pictures (each once, up to [`MAX_FILES`]).
    pub fn add(&mut self, files: Vec<String>) {
        for f in files {
            if self.files.len() >= MAX_FILES {
                break;
            }
            if !self.files.contains(&f) && ImageFormat::from_extension(extension(&f)).is_some_and(|x| x != ImageFormat::Pdf) {
                self.files.push(f);
            }
        }
    }

    /// Adds a change with its command as it starts.
    pub fn push(&mut self, change: Change) {
        let cmd = change.command();
        self.changes.push((change, cmd));
    }

    /// The commands of the changes, in order.
    pub fn steps(&self) -> Vec<(String, Value)> {
        self.changes.iter().filter_map(|(_, c)| c.as_ref()).filter(|(id, _)| !id.is_empty()).map(|(id, p)| (id.to_string(), p.clone())).collect()
    }

    fn running(&self) -> bool {
        self.progress.as_ref().is_some_and(|p| !p.done)
    }
}

fn extension(path: &str) -> &str {
    std::path::Path::new(path).extension().and_then(|x| x.to_str()).unwrap_or("")
}

/// A picture's name without its extension.
fn stem(path: &str) -> String {
    std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// The file a changed picture is saved as, beside the original: the pattern (`{name}` the
/// original's name, `{n}` its place in the batch, and the capture names' `{date}`, `{time}`, …),
/// the format's extension, and ` (2)`, ` (3)`… while that file is there. Never the original.
pub fn out_path(original: &str, pattern: &str, n: u32, format: ImageFormat, at: Stamp, exists: &dyn Fn(&str) -> bool) -> String {
    let name = stem(original);
    let pattern = if pattern.trim().is_empty() { DEFAULT_PATTERN } else { pattern };
    let expanded = screengrabcraft_model::settings::expand_name(&pattern.replace("{name}", &name), at, &name, n);
    let dir = std::path::Path::new(original).parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
    let sep = if original.contains('\\') && !original.contains('/') { "\\" } else { "/" };
    let join = |file: String| if dir.is_empty() { file } else { format!("{dir}{sep}{file}") };
    let ext = format.extension();
    let first = join(format!("{expanded}.{ext}"));
    if first != original && !exists(&first) {
        return first;
    }
    (2..10_000)
        .map(|k| join(format!("{expanded} ({k}).{ext}")))
        .find(|p| p != original && !exists(p))
        .unwrap_or_else(|| join(format!("{expanded} ({n} of the batch).{ext}")))
}

impl Editor {
    /// Opens Batch Changes with these pictures (or adds them to the one open).
    pub fn open_batch(&mut self, files: Vec<String>) {
        match &mut self.batch {
            Some(b) if !b.running() => b.add(files),
            Some(_) => {}
            None => self.batch = Some(Batch::new(files)),
        }
    }

    /// Changes the next picture of a running batch.
    fn batch_step(&mut self, ctx: &egui::Context, b: &mut Batch) {
        let steps = b.steps();
        let Some(p) = b.progress.as_mut() else { return };
        let Some(path) = b.files.get(p.next).cloned() else {
            p.done = true;
            let (n, failed) = (p.written.len(), p.failed.len());
            let words = match failed {
                0 => format!("Changed {n} picture{}", if n == 1 { "" } else { "s" }),
                f => format!("Changed {n}; {f} couldn't be"),
            };
            self.toast(ctx, words);
            // The strip shows the new pictures at once.
            self.recent_at = f64::NEG_INFINITY;
            return;
        };
        p.next += 1;
        let n = u32::try_from(p.next).unwrap_or(u32::MAX);
        let result = (|| -> Result<String, String> {
            let bytes = self.services.read_file(&path)?;
            let picture = screengrabcraft_render::decode(&bytes).map_err(|e| e.to_string())?;
            let changed = screengrabcraft_engine::batch::apply(picture, &steps).map_err(|e| e.to_string())?;
            let format =
                b.format.or_else(|| ImageFormat::from_extension(extension(&path))).filter(|f| FORMATS.contains(f)).unwrap_or(ImageFormat::Png);
            let out = out_path(&path, &b.pattern, n, format, b.stamp, &|p| self.services.file_exists(p));
            let encoded = screengrabcraft_render::encode(&changed, format, crate::autosave::JPEG_QUALITY).map_err(|e| e.to_string())?;
            self.services.write_file(&out, &encoded)?;
            Ok(out)
        })();
        match result {
            Ok(out) => p.written.push(out),
            Err(e) => {
                log::warn!("batch: {} not changed: {e}", file_name(&path));
                p.failed.push((path, e));
            }
        }
        ctx.request_repaint();
    }
}

/// Shows Batch Changes, if it's open, and goes on with it if it's running.
pub fn show(ed: &mut Editor, ctx: &egui::Context) {
    let Some(mut b) = ed.batch.take() else { return };
    if b.running() {
        ed.batch_step(ctx, &mut b);
    }
    let t = Tokens::get(ctx);
    let mut close = false;
    let mut start = false;
    egui::Window::new("Batch Changes")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .min_width(560.0)
        .max_width(560.0)
        .frame(widgets::window_frame(&ctx.global_style()))
        .show(ctx, |ui| {
            if widgets::title_bar(ui, "Batch Changes") && !b.running() {
                close = true;
            }
            widgets::content(ui, |ui| {
                let busy = b.running();
                ui.add_enabled_ui(!busy, |ui| body(ed, &mut b, ui));
                if let Some(p) = &b.progress {
                    ui.add_space(8.0);
                    let total = b.files.len().max(1);
                    ui.add(egui::ProgressBar::new(p.next as f32 / total as f32).text(format!("{} of {}", p.next.min(total), b.files.len())));
                    if p.done {
                        let words = format!("{} written beside the originals.", p.written.len());
                        ui.label(egui::RichText::new(words).color(t.muted));
                        for (path, why) in p.failed.iter().take(6) {
                            ui.label(egui::RichText::new(format!("{}: {why}", file_name(path))).color(t.danger));
                        }
                    }
                }
            });
            let n = b.files.len();
            let label = format!("Change {n} Picture{}", if n == 1 { "" } else { "s" });
            let ready = n > 0 && !b.changes.is_empty() && !b.running();
            let buttons = [FooterButton::primary(&label).enabled(ready), FooterButton::cancel(if b.running() { "Stop" } else { "Close" })];
            match widgets::dialog_footer(ui, &buttons).and_then(|i| buttons.get(i)).map(|b| b.role) {
                Some(Role::Default) => start = true,
                Some(_) => close = true,
                None => {}
            }
        });
    if close {
        if b.running() {
            // Stopped: what's written stays.
            if let Some(p) = b.progress.as_mut() {
                p.done = true;
            }
            ed.batch = Some(b);
        }
        return;
    }
    if start {
        b.stamp = ed.services.now();
        b.progress = Some(Progress::default());
        ctx.request_repaint();
    }
    ed.batch = Some(b);
}

/// The window's settings: the pictures, the changes, how they're saved.
fn body(ed: &mut Editor, b: &mut Batch, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("Pictures ({})", b.files.len())).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add_enabled(!b.files.is_empty(), egui::Button::new("Clear")).clicked() {
                b.files.clear();
                b.progress = None;
            }
            if ed.services.has_paths() && ui.button("Add Pictures\u{2026}").clicked() {
                ed.services.request_pictures();
            }
        });
    });
    egui::ScrollArea::vertical().id_salt("batch-files").max_height(110.0).auto_shrink([false, true]).show(ui, |ui| {
        if b.files.is_empty() {
            ui.label(egui::RichText::new("Add the pictures to change.").color(t.muted));
        }
        let mut gone = None;
        for (i, f) in b.files.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.small_button("\u{00d7}").on_hover_text(format!("Leave {} out", file_name(f))).clicked() {
                    gone = Some(i);
                }
                ui.label(file_name(f)).on_hover_text(f);
            });
        }
        if let Some(i) = gone {
            b.files.remove(i);
        }
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Changes, in order").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button("Add a Change", |ui| {
                for (label, change) in Change::menu() {
                    if ui.button(label).clicked() {
                        b.push(change);
                        ui.close();
                    }
                }
            });
        });
    });
    if b.changes.is_empty() {
        ui.label(egui::RichText::new("Add what to do to each picture: resize it, a border, a shadow…").color(t.muted));
    }
    let mut remove = None;
    let mut up = None;
    let count = b.changes.len();
    for (i, (change, cmd)) in b.changes.iter_mut().enumerate() {
        egui::Frame::NONE.stroke(egui::Stroke::new(1.0, t.border)).corner_radius(6).inner_margin(8).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{}. {}", i + 1, change.title())).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("\u{00d7}").on_hover_text(format!("Take {} out", change.title())).clicked() {
                        remove = Some(i);
                    }
                    // Right to left: shown as up, down, out.
                    if ui.add_enabled(i + 1 < count, egui::Button::new("\u{2193}").small()).on_hover_text("Later").clicked() {
                        up = Some(i + 1);
                    }
                    if ui.add_enabled(i > 0, egui::Button::new("\u{2191}").small()).on_hover_text("Earlier").clicked() {
                        up = Some(i);
                    }
                });
            });
            *cmd = change.ui(ui);
        });
        ui.add_space(4.0);
    }
    if let Some(i) = remove {
        b.changes.remove(i);
    }
    if let Some(i) = up.filter(|i| *i > 0 && *i < b.changes.len()) {
        b.changes.swap(i - 1, i);
    }
    ui.add_space(8.0);
    egui::Grid::new("batch-save").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        ui.label("Save as");
        egui::ComboBox::from_id_salt("batch-format").selected_text(b.format.map_or("Each as it was", ImageFormat::label)).show_ui(ui, |ui| {
            ui.selectable_value(&mut b.format, None, "Each as it was");
            for f in FORMATS {
                ui.selectable_value(&mut b.format, Some(f), f.label());
            }
        });
        ui.end_row();
        ui.label("Name");
        ui.add(egui::TextEdit::singleline(&mut b.pattern).desired_width(280.0));
        ui.end_row();
        ui.label("");
        ui.label(
            egui::RichText::new(
                "{name} is the picture's own name; {n}, {date} and {time} can be used too. Each is saved beside its picture, which stays as it is.",
            )
            .small()
            .color(t.muted),
        );
        ui.end_row();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_changed_picture_is_named_by_the_pattern_beside_the_original_and_never_over_it() {
        let at = Stamp { year: 2026, month: 11, day: 21, hour: 9, minute: 5, second: 0 };
        let none = |_: &str| false;
        assert_eq!(out_path("C:/shots/Login.png", "{name} changed", 1, ImageFormat::Png, at, &none), "C:/shots/Login changed.png");
        assert_eq!(out_path("shots/a.png", "{name}-{n} {date}", 3, ImageFormat::Jpeg, at, &none), "shots/a-3 2026-11-21.jpg");
        assert_eq!(out_path(r"C:\shots\a.png", "{name}", 1, ImageFormat::Webp, at, &none), r"C:\shots\a.webp");
        // The same name and format as the original: never over it.
        assert_eq!(out_path("shots/a.png", "{name}", 1, ImageFormat::Png, at, &none), "shots/a (2).png");
        let taken = |p: &str| p == "shots/a small.png" || p == "shots/a small (2).png";
        assert_eq!(out_path("shots/a.png", "{name} small", 1, ImageFormat::Png, at, &taken), "shots/a small (3).png");
        assert_eq!(out_path("a.png", "  ", 1, ImageFormat::Png, at, &none), "a changed.png", "an empty pattern is the default");
        assert_eq!(out_path("shots/a.png", "../{name}", 1, ImageFormat::Png, at, &none), "shots/..-a.png", "never outside the folder");
    }

    #[test]
    fn only_pictures_are_added_each_once() {
        let mut b = Batch::new(vec!["a.png".into(), "b.JPG".into(), "a.png".into(), "c.sgcraft".into(), "d.pdf".into(), "e".into()]);
        b.add(vec!["b.JPG".into(), "f.webp".into()]);
        assert_eq!(b.files, ["a.png", "b.JPG", "f.webp"]);
    }

    #[test]
    fn every_change_comes_to_a_command() {
        let mut b = Batch::new(Vec::new());
        for (_, c) in Change::menu() {
            b.push(c);
        }
        let steps = b.steps();
        assert_eq!(steps.len(), Change::menu().len());
        let ids: Vec<&str> = steps.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "image.resize",
                "effect.border",
                "effect.shadow",
                "effect.tornEdge",
                "effect.roundCorners",
                "effect.brightness",
                "effect.grayscale",
                "effect.invert",
                "image.trim"
            ]
        );
        for id in ids {
            assert!(screengrabcraft_engine::find(id).is_some(), "{id} is a command");
        }
    }
}
