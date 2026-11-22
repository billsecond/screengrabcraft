//! Arranging the selected annotations, in the Edit menu and the right-click menu: group and
//! ungroup, turn and mirror, line up and spread out, and select every one of a kind.

use egui::Ui;
use serde_json::json;

use crate::editor::Editor;

/// What the items say about the selection.
struct Selection {
    any: bool,
    /// Two or more, not all of one group already.
    groupable: bool,
    grouped: bool,
    /// How many move on their own (a group counts once).
    units: usize,
}

fn selection(ed: &Editor) -> Selection {
    let sel = ed.session.selected();
    let doc = &ed.session.doc;
    let groups: Vec<Option<u64>> = sel.iter().filter_map(|id| doc.get(*id)).map(|o| o.group).collect();
    let one_group = groups.first().is_some_and(|g| g.is_some() && groups.iter().all(|h| h == g));
    Selection {
        any: !sel.is_empty(),
        groupable: sel.len() >= 2 && !one_group,
        grouped: groups.iter().any(Option::is_some),
        units: doc.units(&sel).len(),
    }
}

/// The arranging items; true when one was used (the menu closes).
pub(crate) fn items(ed: &mut Editor, ui: &mut Ui) -> bool {
    let ctx = ui.ctx().clone();
    let s = selection(ed);
    let mut run: Option<(&str, serde_json::Value)> = None;
    if ui.add_enabled(s.groupable, egui::Button::new("Group").shortcut_text("Ctrl+G")).on_hover_text("Select, move and turn them as one").clicked() {
        run = Some(("object.group", json!({})));
    }
    if ui.add_enabled(s.grouped, egui::Button::new("Ungroup").shortcut_text("Ctrl+Shift+G")).clicked() {
        run = Some(("object.ungroup", json!({})));
    }
    ui.add_enabled_ui(s.any, |ui| {
        ui.menu_button("Rotate and Flip", |ui| {
            for (label, params) in [
                ("Rotate Right 90°", json!({"degrees": 90})),
                ("Rotate Left 90°", json!({"degrees": -90})),
                ("Rotate 180°", json!({"degrees": 180})),
                ("Straighten", json!({"angle": 0})),
            ] {
                if ui.button(label).clicked() {
                    run = Some(("object.rotate", params));
                }
            }
            ui.separator();
            if ui.button("Flip Across").on_hover_text("Mirror them left to right, round their middle").clicked() {
                run = Some(("object.flip", json!({"horizontal": true})));
            }
            if ui.button("Flip Up and Down").clicked() {
                run = Some(("object.flip", json!({"horizontal": false})));
            }
        });
        ui.menu_button("Align and Distribute", |ui| {
            let on = if s.units < 2 { "on the picture" } else { "on each other" };
            ui.label(egui::RichText::new(format!("Line them up {on}")).small().weak());
            for (label, to) in
                [("Left", "left"), ("Centre", "center"), ("Right", "right"), ("Top", "top"), ("Middle", "middle"), ("Bottom", "bottom")]
            {
                if ui.button(format!("Align {label}")).clicked() {
                    run = Some(("object.align", json!({"to": to})));
                }
            }
            ui.separator();
            let three = s.units >= 3;
            if ui.add_enabled(three, egui::Button::new("Distribute Across")).on_hover_text("Three or more, the same gap between each").clicked() {
                run = Some(("object.distribute", json!({"horizontal": true})));
            }
            if ui.add_enabled(three, egui::Button::new("Distribute Down")).clicked() {
                run = Some(("object.distribute", json!({"horizontal": false})));
            }
        });
        if ui.button("Select All of This Kind").on_hover_text("Every annotation of the kinds selected").clicked() {
            run = Some(("edit.selectKind", json!({})));
        }
    });
    match run {
        Some((id, params)) => {
            ed.run(&ctx, id, params);
            true
        }
        None => false,
    }
}
