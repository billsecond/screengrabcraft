//! A how-to guide's edits as commands ([`screengrabcraft_model::guide::Guide`]): a step moved,
//! taken out or given new words, the guide's title. The guide window runs them by id; steps are
//! counted from 0.

use screengrabcraft_model::guide::{Guide, MAX_TEXT, MAX_TITLE, clean_text};
use serde_json::{Value, json};

use crate::{EngineError, Result};

/// One guide command.
pub struct GuideCommand {
    pub id: &'static str,
    pub label: &'static str,
    /// What parameters it takes (JSON), for people and agents.
    pub params: &'static str,
    pub run: fn(&mut Guide, &Value) -> Result<Value>,
}

/// Every guide command.
pub const GUIDE_COMMANDS: &[GuideCommand] = &[
    GuideCommand { id: "guide.move", label: "Move Step", params: r#"{"step": n, "to": n}"#, run: move_step },
    GuideCommand { id: "guide.delete", label: "Delete Step", params: r#"{"step": n}"#, run: delete },
    GuideCommand { id: "guide.text", label: "Change Words", params: r#"{"step": n, "text": "…"}"#, run: text },
    GuideCommand { id: "guide.title", label: "Change Title", params: r#"{"title": "…"}"#, run: title },
];

/// Runs a guide command; one given nonsense returns an error and changes nothing.
pub fn run(guide: &mut Guide, id: &str, params: &Value) -> Result<Value> {
    let cmd = GUIDE_COMMANDS.iter().find(|c| c.id == id).ok_or_else(|| EngineError::Unknown(id.to_string()))?;
    let before = guide.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (cmd.run)(guide, params)))
        .unwrap_or_else(|_| Err(EngineError::Panicked(id.to_string())));
    if result.is_err() {
        *guide = before;
    }
    result
}

/// Every guide command, for people and agents.
pub fn describe() -> Value {
    Value::Array(GUIDE_COMMANDS.iter().map(|c| json!({"id": c.id, "label": c.label, "params": c.params})).collect())
}

/// A step of the guide, by its place.
fn step(g: &Guide, p: &Value, key: &str) -> Result<usize> {
    let n = p.get(key).and_then(Value::as_u64).ok_or_else(|| EngineError::Params(format!("`{key}` must be a step's number, from 0")))?;
    usize::try_from(n).ok().filter(|n| *n < g.steps.len()).ok_or_else(|| EngineError::Params(format!("there is no step {n}")))
}

fn words(p: &Value, key: &str) -> Result<String> {
    p.get(key).and_then(Value::as_str).map(str::to_string).ok_or_else(|| EngineError::Params(format!("`{key}` must be text")))
}

fn move_step(g: &mut Guide, p: &Value) -> Result<Value> {
    let (from, to) = (step(g, p, "step")?, step(g, p, "to")?);
    let s = g.steps.remove(from);
    g.steps.insert(to, s);
    Ok(json!({"step": to}))
}

fn delete(g: &mut Guide, p: &Value) -> Result<Value> {
    let i = step(g, p, "step")?;
    let gone = g.steps.remove(i);
    Ok(json!({"picture": gone.picture, "steps": g.steps.len()}))
}

fn text(g: &mut Guide, p: &Value) -> Result<Value> {
    let i = step(g, p, "step")?;
    let t = clean_text(&words(p, "text")?, MAX_TEXT);
    if let Some(s) = g.steps.get_mut(i) {
        s.text = t;
    }
    Ok(json!({"step": i}))
}

fn title(g: &mut Guide, p: &Value) -> Result<Value> {
    g.title = clean_text(&words(p, "title")?, MAX_TITLE).replace('\n', " ");
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use screengrabcraft_model::guide::GuideStep;

    use super::*;

    fn guide(n: usize) -> Guide {
        Guide {
            title: "How to".into(),
            steps: (0..n).map(|i| GuideStep { picture: format!("step-{i:02}.png"), text: format!("Step {i}"), click: None }).collect(),
        }
    }

    fn order(g: &Guide) -> Vec<&str> {
        g.steps.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn steps_are_moved_taken_out_and_reworded_by_commands() {
        let mut g = guide(4);
        run(&mut g, "guide.move", &json!({"step": 3, "to": 0})).unwrap();
        assert_eq!(order(&g), ["Step 3", "Step 0", "Step 1", "Step 2"]);
        run(&mut g, "guide.move", &json!({"step": 0, "to": 3})).unwrap();
        assert_eq!(order(&g), ["Step 0", "Step 1", "Step 2", "Step 3"]);
        let v = run(&mut g, "guide.delete", &json!({"step": 1})).unwrap();
        assert_eq!(v["picture"], json!("step-01.png"), "the host knows which picture went");
        run(&mut g, "guide.text", &json!({"step": 0, "text": "Open Settings\n\u{7}"})).unwrap();
        assert_eq!(order(&g), ["Open Settings\n", "Step 2", "Step 3"]);
        run(&mut g, "guide.title", &json!({"title": "Log\nin"})).unwrap();
        assert_eq!(g.title, "Log in");
    }

    #[test]
    fn hostile_params_never_panic_or_change_the_guide() {
        let junk = [
            json!(null),
            json!("x"),
            json!({}),
            json!({"step": -1, "to": 2}),
            json!({"step": 99, "to": 0}),
            json!({"step": 1, "to": 1e300}),
            json!({"step": 1.5, "text": 7, "title": ["x"]}),
            json!({"step": u64::MAX, "to": u64::MAX}),
        ];
        for c in GUIDE_COMMANDS {
            for p in &junk {
                for n in [0, 3] {
                    let mut g = guide(n);
                    let before = g.clone();
                    match run(&mut g, c.id, p) {
                        Ok(_) => {}
                        Err(EngineError::Panicked(id)) => panic!("{id} panicked on {p}"),
                        Err(_) => assert_eq!(g, before, "{} with {p} failed but changed the guide", c.id),
                    }
                }
            }
        }
        assert_eq!(describe().as_array().map(Vec::len), Some(GUIDE_COMMANDS.len()));
    }
}
