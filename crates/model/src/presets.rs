//! Effect presets: an effect's settings kept under a name, applied with one click. A preset is
//! one effect or a few one after another (a border, then a shadow); the ready-made ones are here,
//! the owner's own are kept in the editor's settings.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// One effect of a preset: its command (`effect.border`) and that command's parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectStep {
    pub effect: String,
    #[serde(default)]
    pub params: Value,
}

/// Effects kept under a name, applied together with one click (one undo step).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectPreset {
    pub name: String,
    pub steps: Vec<EffectStep>,
}

/// The most presets of their own the owner keeps.
pub const MAX_PRESETS: usize = 32;
/// The most effects in one preset.
pub const MAX_STEPS: usize = 8;
/// The longest a preset's name is, in characters.
pub const MAX_PRESET_NAME: usize = 40;

/// Whether `id` is an effect a preset can hold: one of the picture's effects, not a preset itself.
pub fn is_effect(id: &str) -> bool {
    id.starts_with("effect.") && id != "effect.preset" && id.len() <= 64
}

/// A preset's name made fit to keep: trimmed, without control characters, at most
/// [`MAX_PRESET_NAME`] characters; none when nothing is left.
pub fn preset_name(name: &str) -> Option<String> {
    let kept: String = name.chars().filter(|c| !c.is_control()).collect::<String>().trim().chars().take(MAX_PRESET_NAME).collect();
    let kept = kept.trim().to_string();
    (!kept.is_empty()).then_some(kept)
}

impl EffectPreset {
    /// The preset made fit to keep: its name tidied, only effects it can hold (their parameters an
    /// object each), at most [`MAX_STEPS`] of them; none when no name or no effect is left.
    pub fn clamped(self) -> Option<EffectPreset> {
        let name = preset_name(&self.name)?;
        let steps: Vec<EffectStep> = self
            .steps
            .into_iter()
            .filter(|s| is_effect(&s.effect))
            .map(|s| EffectStep { params: if s.params.is_object() { s.params } else { json!({}) }, ..s })
            .take(MAX_STEPS)
            .collect();
        (!steps.is_empty()).then_some(EffectPreset { name, steps })
    }
}

fn step(effect: &str, params: Value) -> EffectStep {
    EffectStep { effect: effect.to_string(), params }
}

/// The ready-made presets, shown before the owner's own.
pub fn ready_made() -> Vec<EffectPreset> {
    let preset = |name: &str, steps: Vec<EffectStep>| EffectPreset { name: name.to_string(), steps };
    vec![
        preset("Soft Shadow", vec![step("effect.shadow", json!({"offset": 6, "softness": 16, "color": "#00000060"}))]),
        preset(
            "Photo Frame",
            vec![step("effect.border", json!({"width": 14, "color": "#FFFFFF"})), step("effect.shadow", json!({"offset": 6, "softness": 12}))],
        ),
        preset("Rounded Card", vec![step("effect.roundCorners", json!({"radius": 18})), step("effect.shadow", json!({"offset": 4, "softness": 14}))]),
        preset(
            "Torn Paper",
            vec![
                step("effect.tornEdge", json!({"bottom": true, "depth": 14})),
                step("effect.shadow", json!({"offset": 5, "softness": 8, "color": "#00000066"})),
            ],
        ),
        preset("Old Photo", vec![step("effect.sepia", json!({"amount": 85})), step("effect.fadedEdge", json!({"width": 40}))]),
        preset("Faded Edges", vec![step("effect.fadedEdge", json!({"width": 30}))]),
        preset("Thin Line", vec![step("effect.border", json!({"width": 1, "color": "#BDBDBD"}))]),
        preset("Black and White", vec![step("effect.grayscale", json!({})), step("effect.brightness", json!({"contrast": 20}))]),
    ]
}

/// Whether `name` is a ready-made preset's.
pub fn is_ready_made(name: &str) -> bool {
    ready_made().iter().any(|p| p.name.eq_ignore_ascii_case(name.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_hold_only_effects_and_have_names() {
        let all = ready_made();
        assert!(all.len() >= 6);
        for (i, p) in all.iter().enumerate() {
            assert_eq!(p.clone().clamped().as_ref(), Some(p), "{} is fit to keep", p.name);
            assert!(all.iter().skip(i + 1).all(|q| q.name != p.name));
        }
        assert!(is_ready_made(" soft shadow") && !is_ready_made("Mine"));
        let odd = EffectPreset {
            name: " \u{1b}Mine ".into(),
            steps: vec![
                step("image.crop", json!({})),
                step("effect.preset", json!({})),
                step("effect.border", json!(7)),
                step("effect.sepia", json!({"amount": 20})),
            ],
        };
        let kept = odd.clamped().unwrap();
        assert_eq!(kept.name, "Mine");
        assert_eq!(kept.steps, vec![step("effect.border", json!({})), step("effect.sepia", json!({"amount": 20}))]);
        let none = EffectPreset { name: "x".into(), steps: vec![step("image.resize", json!({}))] };
        assert_eq!(none.clamped(), None, "nothing it can hold");
        let many = EffectPreset { name: "x".into(), steps: vec![step("effect.invert", json!({})); 20] };
        assert_eq!(many.clamped().map(|p| p.steps.len()), Some(MAX_STEPS));
        assert_eq!(preset_name("  "), None);
        assert_eq!(preset_name(&"y".repeat(99)).map(|n| n.chars().count()), Some(MAX_PRESET_NAME));
    }
}
