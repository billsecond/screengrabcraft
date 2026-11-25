//! Words that stay inside their box, in the Recorder and in every dialog of the editor: drawn
//! with the interface's own font at display scales 1, 1.5 and 2, with long words (device names,
//! messages, titles), nothing runs past the edge of what it's on, off the window or into other
//! words, and no two controls are over each other (`crate::fit`).

use egui::accesskit::Role;
use egui::{Rect, Vec2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT as _, Queryable as _};
use screengrabcraft_model::{Settings, Theme};

use crate::dialogs::Dialog;
use crate::editor::Editor;
use crate::fit;
use crate::recorder::{self, CameraPreview, MicLevel, RecorderDevices, RecorderMonitor, RecorderView};
use crate::widgets;

/// The harness's margin round what it shows.
const EDGE: Vec2 = vec2(16.0, 16.0);
/// The display scales everything is checked at.
const SCALES: [f32; 3] = [1.0, 1.5, 2.0];

/// Every control and label the harness shows, named, and where it is.
fn controls<S>(h: &Harness<'_, S>) -> Vec<(String, Rect)> {
    const SHOWN: [Role; 13] = [
        Role::Button,
        Role::CheckBox,
        Role::Label,
        Role::ProgressIndicator,
        Role::ComboBox,
        Role::Image,
        Role::Link,
        Role::SpinButton,
        Role::TextInput,
        Role::Slider,
        Role::RadioButton,
        Role::Switch,
        Role::ColorWell,
    ];
    h.query_all(By::new().predicate(|n| SHOWN.contains(&n.role())))
        .map(|n| {
            let a = n.accesskit_node();
            (a.label().or_else(|| a.value()).unwrap_or_default(), n.rect())
        })
        .filter(|(_, r)| r.is_positive())
        .collect()
}

/// What doesn't fit in what the harness last drew.
fn check<S>(h: &Harness<'_, S>) -> Vec<String> {
    let mut found = fit::spills(&h.output().shapes, h.ctx.content_rect());
    found.extend(fit::crowded(&controls(h)));
    found
}

fn shot<S>(h: &mut Harness<'_, S>, name: &str, scale: f32) {
    if let Ok(dir) = std::env::var("SGC_SHOTS")
        && let Ok(img) = h.render()
    {
        let _ = img.save(format!("{dir}/{name}@{scale}.png"));
    }
}

// ---- the Recorder ----

struct Rec {
    settings: Settings,
    view: RecorderView,
    devices: RecorderDevices,
    mic: MicLevel,
    sound: MicLevel,
    camera: CameraPreview,
}

const CAMERA: &str = "Logitech BRIO 4K Stream Edition Webcam";
const CAMERA_FAILED: &str = "The camera didn't start: the device is in use by another program, or Windows' privacy settings don't let desktop apps use the camera. Close the other program and try again.";

/// A computer whose devices have the long names real ones have.
fn long_devices() -> RecorderDevices {
    RecorderDevices {
        monitors: vec![
            RecorderMonitor {
                name: "Dell UltraSharp U3224KB 6K Thunderbolt Hub Monitor".into(),
                x: 0,
                y: 0,
                width: 6144,
                height: 3456,
                primary: true,
            },
            RecorderMonitor { name: "Built-in Retina Display".into(), x: -1920, y: 0, width: 1920, height: 1200, primary: false },
        ],
        all_screens: Some((8064, 3456)),
        microphones: vec![
            "Microphone Array (Intel® Smart Sound Technology for Digital Microphones)".into(),
            "Headset Microphone (Jabra Evolve2 65 Bluetooth Hands-Free AG Audio)".into(),
            format!("Microphone ({CAMERA})"),
            "Line In (Focusrite Scarlett 2i2 4th Gen USB Audio Interface)".into(),
        ],
        default_microphone: Some(format!("Microphone ({CAMERA})")),
        cameras: Some(vec![(0, CAMERA.into()), (1, "Integrated Webcam with Windows Hello IR Face Recognition".into())]),
        camera_supported: true,
        system_sound: true,
        system_sound_platform: true,
        speakers: vec!["Speakers (Realtek(R) High Definition Audio with Dolby Atmos for Headphones)".into()],
        default_speaker: Some("Speakers (Realtek(R) High Definition Audio with Dolby Atmos for Headphones)".into()),
    }
}

/// The Recorder at its fullest: every input on, two more microphones and another camera, the
/// microphone the camera's own and hearing only silence, the camera not starting, and the
/// computer's sound not heard.
fn fullest() -> Rec {
    let mut settings = Settings::default();
    settings.recorder.area = screengrabcraft_model::RecordArea::Screen;
    let p = &mut settings.production;
    (p.camera, p.microphone, p.system_sound) = (true, true, true);
    p.microphone_name = None;
    p.more_microphones = vec![
        "Headset Microphone (Jabra Evolve2 65 Bluetooth Hands-Free AG Audio)".into(),
        "Line In (Focusrite Scarlett 2i2 4th Gen USB Audio Interface)".into(),
    ];
    p.more_cameras = vec![1];
    Rec {
        settings,
        view: {
            let mut v = RecorderView::default();
            v.hint = Some("Ctrl+Shift+F9 pauses, Ctrl+Shift+F10 stops".into());
            v
        },
        devices: long_devices(),
        mic: MicLevel::Silent,
        sound: MicLevel::Failed("the sound device didn't start".into()),
        camera: CameraPreview::Failed(CAMERA_FAILED.into()),
    }
}

/// The Recorder drawn as the service draws it, its window the size it asks for. The harness,
/// the window's size, and what doesn't fit.
fn recorder_fitted(rec: Rec, scale: f32) -> (Harness<'static, Rec>, Vec2, Vec<String>) {
    let mut size = recorder::size(&rec.settings.production);
    let mut h = Harness::builder().with_size(size + EDGE).with_pixels_per_point(scale).build_ui_state(
        |ui, r: &mut Rec| {
            crate::theme::apply(ui.ctx(), Theme::Light);
            let _ =
                recorder::recorder_ui(ui, &mut r.view, &mut r.settings.recorder, &mut r.settings.production, &r.devices, &r.mic, &r.sound, &r.camera);
        },
        rec,
    );
    crate::install_fonts(&h.ctx);
    h.run_steps(2);
    for _ in 0..6 {
        let wanted = recorder::window_size(&h.ctx, &h.state().settings.production);
        if wanted == size {
            break;
        }
        size = wanted;
        h.set_size(size + EDGE);
        h.run_steps(2);
    }
    let mut found = check(&h);
    let last = recorder::window_size(&h.ctx, &h.state().settings.production);
    if last != size {
        found.push(format!("the window keeps growing: {size:?}, then {last:?}"));
    }
    (h, size, found)
}

/// Every card's words stay on the card at every scale, however much a card says: the cards and
/// the window grow to the card that says most; with little to say, the window is its own size.
#[test]
fn the_recorders_words_stay_on_their_cards_at_every_scale() {
    let mut wrong = Vec::new();
    for scale in SCALES {
        let (mut h, _, found) = recorder_fitted(fullest(), scale);
        wrong.extend(found.into_iter().map(|f| format!("fullest @{scale}: {f}")));
        h.get_by_label_contains("let desktop apps use the microphone");
        h.get_by_label_contains("That's the camera's own microphone.");
        shot(&mut h, "recorder-fullest", scale);
        // The same warnings with no further microphone or camera, whose rows made room: the
        // cards and the window grow to them.
        let mut rec = fullest();
        rec.settings.production.more_microphones.clear();
        rec.settings.production.more_cameras.clear();
        let (mut h, size, found) = recorder_fitted(rec, scale);
        wrong.extend(found.into_iter().map(|f| format!("warnings @{scale}: {f}")));
        assert!(size.y > recorder::SIZE.y, "it grew: {size:?}");
        shot(&mut h, "recorder-warnings", scale);
        // No microphone at all, every monitor.
        let mut rec = fullest();
        rec.devices.microphones.clear();
        rec.devices.default_microphone = None;
        rec.settings.production.more_microphones.clear();
        rec.mic = MicLevel::Off;
        rec.settings.recorder.area = screengrabcraft_model::RecordArea::AllScreens;
        let (mut h, _, found) = recorder_fitted(rec, scale);
        wrong.extend(found.into_iter().map(|f| format!("no microphone @{scale}: {f}")));
        shot(&mut h, "recorder-no-microphone", scale);
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    // The defaults: the window as it's made.
    let plain = Rec {
        settings: Settings::default(),
        view: RecorderView::default(),
        devices: RecorderDevices { cameras: Some(vec![(0, "Webcam".into())]), ..long_devices() },
        mic: MicLevel::Level(0.3),
        sound: MicLevel::Level(0.2),
        camera: CameraPreview::Off,
    };
    let (_, size, found) = recorder_fitted(plain, 1.0);
    assert!(found.is_empty(), "{found:?}");
    assert_eq!(size, recorder::size(&Settings::default().production));
}

// ---- the editor's dialogs ----

/// Each of the editor's windows opened by `open` and drawn alone by `draw`, at every scale: what
/// doesn't fit.
fn windows_check(name: &str, open: impl Fn(&mut Editor), draw: fn(&mut Editor, &egui::Context)) -> Vec<String> {
    let mut wrong = Vec::new();
    for scale in SCALES {
        let mut ed = crate::tests::editor();
        open(&mut ed);
        let mut h = Harness::builder().with_size(vec2(1100.0, 1300.0)).with_pixels_per_point(scale).build_ui_state(
            move |ui, ed: &mut Editor| {
                crate::theme::apply(ui.ctx(), Theme::Light);
                let ctx = ui.ctx().clone();
                draw(ed, &ctx);
            },
            ed,
        );
        crate::install_fonts(&h.ctx);
        h.run_steps(30);
        wrong.extend(check(&h).into_iter().map(|f| format!("{name} @{scale}: {f}")));
        shot(&mut h, name, scale);
    }
    wrong
}

/// Every dialog of the editor (an effect's settings, the shortcuts, About) keeps its words in
/// it at every scale.
#[test]
fn every_dialog_keeps_its_words_inside_at_every_scale() {
    let doc = screengrabcraft_model::Document::new(screengrabcraft_model::Raster::filled(40, 30, screengrabcraft_model::Color::WHITE).unwrap());
    let all = [
        Dialog::resize(&doc),
        Dialog::canvas(),
        Dialog::border(),
        Dialog::shadow(),
        Dialog::torn(),
        Dialog::rotate_any(),
        Dialog::round(),
        Dialog::brightness(),
        Dialog::wavy(),
        Dialog::faded(),
        Dialog::bevel(),
        Dialog::perspective(),
        Dialog::curl(),
        Dialog::reflection(),
        Dialog::hue(),
        Dialog::replace(),
        Dialog::sharpen(),
        Dialog::blur(),
        Dialog::watermark(),
        Dialog::details(),
        Dialog::Shortcuts,
        Dialog::VideoShortcuts,
        Dialog::About,
    ];
    let mut wrong = Vec::new();
    for d in all {
        let name = format!("dialog-{}", d.title().to_lowercase().replace([' ', ','], "-"));
        wrong.extend(windows_check(&name, move |ed| crate::dialogs::open(ed, d.clone()), crate::dialogs::show));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The editor's other windows (pictures put together, a template, the tool bar's choice, a QR
/// code, the personal details found, batch changes to files with long names) keep their words in
/// them at every scale.
#[test]
fn the_editors_other_windows_keep_their_words_inside_at_every_scale() {
    let mut wrong = Vec::new();
    wrong.extend(windows_check("compose-combine", |ed| ed.open_compose(crate::compose::ComposeKind::Combine), crate::compose::show));
    wrong.extend(windows_check("compose-template", |ed| ed.open_compose(crate::compose::ComposeKind::Template), crate::compose::show));
    wrong.extend(windows_check(
        "tool-bar",
        |ed| ed.customizing_bar = Some(crate::toolbar::BarDraft::of(&ed.settings.tool_bar)),
        crate::toolbar::show,
    ));
    wrong.extend(windows_check(
        "qr-make",
        |ed| ed.qr.making = Some("https://example.com/a/very/long/address/that/goes/on/and/on?with=a&query=string&and=more".into()),
        crate::qr::show,
    ));
    wrong.extend(windows_check(
        "batch",
        |ed| {
            ed.open_batch(vec![
                "C:/Users/someone/Pictures/ScreenGrabCraft/Capture 2026-11-25 09.41.07 of the quarterly report window, second monitor.png".into(),
                "C:/Users/someone/Pictures/ScreenGrabCraft/Capture 2026-11-25 09.42.13.png".into(),
            ])
        },
        crate::batch::show,
    ));
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

// ---- the parts every dialog is made of ----

/// A title too long for its bar wraps clear of the X, and the bar grows to it.
#[test]
fn a_long_title_wraps_clear_of_the_close_control() {
    for scale in SCALES {
        let mut h = Harness::builder().with_size(vec2(300.0, 160.0)).with_pixels_per_point(scale).build_ui(|ui| {
            crate::theme::apply(ui.ctx(), Theme::Light);
            egui::Frame::NONE.fill(egui::Color32::WHITE).show(ui, |ui| {
                let _ = widgets::title_bar(ui, "Hue, Saturation and Gamma, with a title far too long for its window");
            });
        });
        crate::install_fonts(&h.ctx);
        h.run_steps(2);
        let found = check(&h);
        assert!(found.is_empty(), "{scale}: {found:?}");
        let x = h.get_by_label("Close window").rect();
        let words = h
            .output()
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text().starts_with("Hue") => Some(t.visual_bounding_rect()),
                _ => None,
            })
            .next()
            .unwrap();
        assert!(words.max.x < x.min.x && words.height() > 30.0, "two lines, clear of the X: {words:?} {x:?}");
        shot(&mut h, "title-bar-long", scale);
    }
}

/// Buttons that don't fit across a footer go on more rows, each at the right, rather than past
/// its edge; ones that fit stay on one row.
#[test]
fn a_footer_too_narrow_for_its_buttons_wraps_them() {
    let width = |i: usize| [90.0, 120.0, 100.0, 88.0].get(i).copied().unwrap_or(88.0);
    assert_eq!(widgets::footer_rows(&[3], &[0, 1, 2], width, 500.0), vec![(vec![3], vec![0, 1, 2])]);
    assert_eq!(widgets::footer_rows(&[3], &[0, 1, 2], width, 250.0), vec![(vec![3], vec![]), (vec![], vec![0, 1]), (vec![], vec![2])]);
    assert_eq!(widgets::footer_rows(&[], &[0, 1], width, 60.0), vec![(vec![], vec![0]), (vec![], vec![1])]);
    let buttons = [
        widgets::FooterButton::primary("Apply to Every Picture"),
        widgets::FooterButton::plain("Save the Settings as a Preset"),
        widgets::FooterButton::cancel("Cancel"),
    ];
    for scale in SCALES {
        let mut h = Harness::builder().with_size(vec2(380.0, 200.0)).with_pixels_per_point(scale).build_ui(move |ui| {
            crate::theme::apply(ui.ctx(), Theme::Light);
            let _ = widgets::dialog_footer(ui, &buttons);
        });
        crate::install_fonts(&h.ctx);
        h.run_steps(2);
        let found = check(&h);
        assert!(found.is_empty(), "{scale}: {found:?}");
        for b in &buttons {
            let r = h.get_by_label(b.label).rect();
            assert!(r.max.x <= 380.0 && r.min.x >= 0.0, "{} at {scale}: {r:?}", b.label);
        }
        shot(&mut h, "footer-narrow", scale);
    }
}
