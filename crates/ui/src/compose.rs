//! Combine Pictures and Templates: the picture open and others (from the recent captures or from
//! files the host reads) put together side by side, stacked or in a grid, or laid out on a frame
//! with a title and the words of each step (`image.combine`, `image.template`).

use egui::{Color32, Ui};
use screengrabcraft_model::Raster;
use serde_json::{Value, json};

use crate::editor::Editor;
use crate::services::OpenedFile;
use crate::theme::Tokens;
use crate::widgets::{self, FooterButton, Role};

/// Which of the two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComposeKind {
    Combine,
    Template,
}

/// The window's settings, and the pictures put with the one open.
#[derive(Clone, Debug)]
pub struct Compose {
    pub kind: ComposeKind,
    pub pictures: Vec<(String, Raster)>,
    pub layout: &'static str,
    pub gap: u32,
    pub background: Color32,
    pub accent: Color32,
    pub title: String,
    /// The words of each step: the picture open first.
    pub steps: Vec<String>,
}

/// The most pictures put together.
const MOST: usize = 16;

const COMBINE_LAYOUTS: [(&str, &str); 3] = [("across", "Side by side"), ("down", "Stacked"), ("grid", "In a grid")];
const TEMPLATE_LAYOUTS: [(&str, &str); 4] =
    [("stepsAcross", "Steps in a row"), ("stepsDown", "Steps down the page"), ("beforeAfter", "Before and after"), ("grid", "Steps in a grid")];

impl Compose {
    pub fn new(kind: ComposeKind) -> Compose {
        Compose {
            kind,
            pictures: Vec::new(),
            layout: if kind == ComposeKind::Combine { "across" } else { "stepsAcross" },
            gap: 16,
            background: if kind == ComposeKind::Combine { Color32::WHITE } else { Color32::from_rgb(0xF4, 0xF6, 0xFA) },
            accent: Color32::from_rgb(0x1E, 0x88, 0xE5),
            title: String::new(),
            steps: Vec::new(),
        }
    }

    fn title(&self) -> &'static str {
        match self.kind {
            ComposeKind::Combine => "Combine Pictures",
            ComposeKind::Template => "Template",
        }
    }

    /// The command and its parameters.
    fn command(&self) -> (&'static str, Value) {
        let hex = |c: Color32| crate::from_color32(c).to_hex();
        match self.kind {
            ComposeKind::Combine => ("image.combine", json!({"layout": self.layout, "gap": self.gap, "background": hex(self.background)})),
            ComposeKind::Template => (
                "image.template",
                json!({"layout": self.layout, "title": self.title, "steps": self.steps, "background": hex(self.background), "accent": hex(self.accent)}),
            ),
        }
    }

    /// Adds a picture (none past the most).
    pub fn add(&mut self, name: String, picture: Raster) {
        if self.pictures.len() < MOST {
            self.pictures.push((name, picture));
        }
    }
}

impl Editor {
    /// Opens Combine Pictures or Template.
    pub(crate) fn open_compose(&mut self, kind: ComposeKind) {
        self.compose = Some(Compose::new(kind));
    }

    /// Picture files picked to put with this one.
    pub(crate) fn compose_files(&mut self, ctx: &egui::Context, files: Vec<OpenedFile>) {
        for f in files {
            match screengrabcraft_render::decode(&f.bytes) {
                Ok(pic) => {
                    if let Some(c) = &mut self.compose {
                        c.add(f.name.clone(), pic);
                    }
                }
                Err(e) => self.error(ctx, format!("{}: {e}", f.name)),
            }
        }
    }

    /// Puts the pictures together: they are lent to the command for as long as it runs.
    fn apply_compose(&mut self, ctx: &egui::Context, c: &Compose) -> bool {
        self.session.lent = c.pictures.iter().map(|(_, p)| p.clone()).collect();
        let (id, params) = c.command();
        let done = self.run(ctx, id, params).is_some();
        self.session.lent.clear();
        if done {
            self.area = None;
            self.view.fit_pending = true;
        }
        done
    }
}

/// The window, while it is open.
pub fn show(ed: &mut Editor, ctx: &egui::Context) {
    let Some(mut c) = ed.compose.take() else { return };
    let mut close = false;
    let mut apply = false;
    let t = Tokens::get(ctx);
    egui::Window::new(c.title())
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .min_width(440.0)
        .max_width(440.0)
        .frame(widgets::window_frame(&ctx.global_style()))
        .show(ctx, |ui| {
            if widgets::title_bar(ui, c.title()) {
                close = true;
            }
            widgets::content(ui, |ui| body(ed, &mut c, ui, &t));
            let ready = !c.pictures.is_empty() || (c.kind == ComposeKind::Template && c.layout != "beforeAfter");
            let buttons = [FooterButton::primary(if c.kind == ComposeKind::Combine { "Combine" } else { "Make" }), FooterButton::cancel("Cancel")];
            match widgets::dialog_footer(ui, &buttons).and_then(|i| buttons.get(i)).map(|b| b.role) {
                Some(Role::Default) if ready => apply = true,
                Some(Role::Default) => ed.error(ctx, "Add a picture to put with this one first"),
                Some(_) => close = true,
                None => {}
            }
        });
    if apply && ed.apply_compose(ctx, &c) {
        return;
    }
    if !close {
        ed.compose = Some(c);
    }
}

fn body(ed: &mut Editor, c: &mut Compose, ui: &mut Ui, t: &Tokens) {
    let ctx = ui.ctx().clone();
    ui.label(egui::RichText::new("This picture, then:").strong());
    let mut remove = None;
    for (i, (name, pic)) in c.pictures.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("{}. {name}", i + 2));
            ui.label(egui::RichText::new(format!("{} × {}", pic.width(), pic.height())).color(t.muted));
            if ui.small_button("Remove").on_hover_text("Leave this picture out").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove
        && i < c.pictures.len()
    {
        c.pictures.remove(i);
    }
    if c.pictures.is_empty() {
        ui.label(egui::RichText::new("No other picture yet.").color(t.muted));
    }
    let recent: Vec<(String, String)> = ed
        .recent
        .iter()
        .filter(|r| !crate::strip::is_video(&r.path) && Some(&r.path) != ed.picture_path.as_ref())
        .take(24)
        .map(|r| (r.path.clone(), r.name.clone()))
        .collect();
    let mut pick: Option<(String, String)> = None;
    let mut files = false;
    ui.horizontal_wrapped(|ui| {
        ui.add_enabled_ui(!recent.is_empty(), |ui| {
            ui.menu_button("Add a Recent Capture", |ui| {
                for (path, name) in &recent {
                    if ui.button(name).clicked() {
                        pick = Some((path.clone(), name.clone()));
                        ui.close();
                    }
                }
            });
        });
        files = ui
            .add_enabled(ed.services.has_paths(), egui::Button::new("Add Files…"))
            .on_hover_text("Pick picture files to put with this one")
            .clicked();
    });
    if let Some((path, name)) = pick {
        match ed.services.read_file(&path).and_then(|b| screengrabcraft_render::decode(&b).map_err(|e| e.to_string())) {
            Ok(pic) => c.add(name, pic),
            Err(e) => ed.error(&ctx, format!("{name}: {e}")),
        }
    }
    if files {
        ed.adding_pictures = true;
        ed.services.request_open();
    }
    ui.add_space(10.0);
    egui::Grid::new("compose").num_columns(2).spacing([16.0, 10.0]).min_col_width(80.0).show(ui, |ui| {
        ui.label("Layout");
        let layouts: &[(&'static str, &str)] = if c.kind == ComposeKind::Combine { &COMBINE_LAYOUTS } else { &TEMPLATE_LAYOUTS };
        let now = layouts.iter().find(|(k, _)| *k == c.layout).map_or("", |(_, l)| *l);
        egui::ComboBox::from_id_salt("compose-layout").selected_text(now).show_ui(ui, |ui| {
            for (key, label) in layouts {
                ui.selectable_value(&mut c.layout, *key, *label);
            }
        });
        ui.end_row();
        match c.kind {
            ComposeKind::Combine => {
                ui.label("Gap");
                ui.add(egui::Slider::new(&mut c.gap, 0..=120).suffix(" px"));
                ui.end_row();
                ui.label("Behind");
                egui::color_picker::color_edit_button_srgba(ui, &mut c.background, egui::color_picker::Alpha::BlendOrAdditive);
                ui.end_row();
            }
            ComposeKind::Template => {
                ui.label("Title");
                ui.add(egui::TextEdit::singleline(&mut c.title).desired_width(260.0).hint_text("How to…"));
                ui.end_row();
                let n = c.pictures.len() + 1;
                c.steps.resize(n, String::new());
                for (i, words) in c.steps.iter_mut().enumerate() {
                    ui.label(format!("Step {}", i + 1));
                    ui.add(egui::TextEdit::singleline(words).desired_width(260.0).hint_text(if i == 0 { "this picture" } else { "" }));
                    ui.end_row();
                }
                ui.label("Colours");
                ui.horizontal(|ui| {
                    egui::color_picker::color_edit_button_srgba(ui, &mut c.background, egui::color_picker::Alpha::Opaque).on_hover_text("The page");
                    egui::color_picker::color_edit_button_srgba(ui, &mut c.accent, egui::color_picker::Alpha::Opaque)
                        .on_hover_text("The step numbers");
                });
                ui.end_row();
            }
        }
    });
    ui.add_space(6.0);
    let note = match c.kind {
        ComposeKind::Combine => "The annotations stay on this picture; the others join it as they are.",
        ComposeKind::Template => "Everything on the page stays an annotation to change, move or take away. Undo puts the picture back.",
    };
    ui.label(egui::RichText::new(note).small().color(t.muted));
}
