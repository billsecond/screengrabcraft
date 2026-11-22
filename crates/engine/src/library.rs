//! The captures' library as commands: tags, favourites and where a capture was taken from
//! ([`screengrabcraft_model::Library`]). The editor's All captures and the capture service run
//! them by id, like every other action; the host keeps the library's file.
//!
//! Every command names its capture by `path`, and answers what's now known of it.

use screengrabcraft_model::library::{CaptureInfo, Library, clean_source, clean_tag};
use serde_json::{Value, json};

use crate::{EngineError, Result};

/// One library command.
pub struct LibraryCommand {
    pub id: &'static str,
    pub label: &'static str,
    /// What parameters it takes (JSON), for people and agents.
    pub params: &'static str,
    pub run: fn(&mut Library, &Value) -> Result<Value>,
}

/// Every library command.
pub const LIBRARY_COMMANDS: &[LibraryCommand] = &[
    LibraryCommand { id: "library.tag", label: "Add Tag", params: r#"{"path": "…", "tag": "…"}"#, run: tag },
    LibraryCommand { id: "library.untag", label: "Remove Tag", params: r#"{"path": "…", "tag": "…"}"#, run: untag },
    LibraryCommand {
        id: "library.favorite",
        label: "Favorite",
        params: r#"{"path": "…", "on"?: true} (without "on", it's turned the other way)"#,
        run: favorite,
    },
    LibraryCommand {
        id: "library.source",
        label: "Taken From",
        params: r#"{"path": "…", "program"?: "…", "title"?: "…"}: the program and window a capture was taken from"#,
        run: source,
    },
    LibraryCommand {
        id: "library.forget", label: "Forget", params: r#"{"path": "…"}: everything known of a capture (it was deleted)"#, run: forget
    },
];

/// Runs a library command; one given nonsense returns an error and changes nothing.
pub fn run(lib: &mut Library, id: &str, params: &Value) -> Result<Value> {
    let cmd = LIBRARY_COMMANDS.iter().find(|c| c.id == id).ok_or_else(|| EngineError::Unknown(id.to_string()))?;
    let before = lib.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (cmd.run)(lib, params)))
        .unwrap_or_else(|_| Err(EngineError::Panicked(id.to_string())));
    if result.is_err() {
        *lib = before;
    }
    result
}

/// Every library command, for people and agents.
pub fn describe() -> Value {
    Value::Array(LIBRARY_COMMANDS.iter().map(|c| json!({"id": c.id, "label": c.label, "params": c.params})).collect())
}

fn path(p: &Value) -> Result<String> {
    match p.get("path").and_then(Value::as_str) {
        Some(s) if !s.trim().is_empty() && s.len() <= 4096 => Ok(s.to_string()),
        _ => Err(EngineError::Params("`path` must name a capture".to_string())),
    }
}

fn text(p: &Value, key: &str) -> Result<Option<String>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_str().map(|s| Some(s.to_string())).ok_or_else(|| EngineError::Params(format!("`{key}` must be text"))),
    }
}

/// What's known of the capture now, as the answer.
fn answer(lib: &Library, path: &str) -> Value {
    let info = lib.get(path);
    json!({"path": path, "tags": info.tags, "favorite": info.favorite, "program": info.program, "title": info.title})
}

fn change(lib: &mut Library, path: &str, f: impl FnOnce(&mut CaptureInfo)) -> Value {
    let mut info = lib.get(path);
    f(&mut info);
    lib.set(path, info);
    answer(lib, path)
}

fn tag_param(p: &Value) -> Result<String> {
    text(p, "tag")?.as_deref().and_then(clean_tag).ok_or_else(|| EngineError::Params("`tag` must be a word or a few".to_string()))
}

fn tag(lib: &mut Library, p: &Value) -> Result<Value> {
    let (path, tag) = (path(p)?, tag_param(p)?);
    if lib.get(&path).tags.len() >= screengrabcraft_model::library::MAX_TAGS {
        return Err(EngineError::Failed(format!("a capture has at most {} tags", screengrabcraft_model::library::MAX_TAGS)));
    }
    Ok(change(lib, &path, |i| {
        i.add_tag(&tag);
    }))
}

fn untag(lib: &mut Library, p: &Value) -> Result<Value> {
    let (path, tag) = (path(p)?, tag_param(p)?);
    Ok(change(lib, &path, |i| {
        i.remove_tag(&tag);
    }))
}

fn favorite(lib: &mut Library, p: &Value) -> Result<Value> {
    let path = path(p)?;
    let on = match p.get("on") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_bool().ok_or_else(|| EngineError::Params("`on` must be true or false".to_string()))?),
    };
    Ok(change(lib, &path, |i| i.favorite = on.unwrap_or(!i.favorite)))
}

fn source(lib: &mut Library, p: &Value) -> Result<Value> {
    let path = path(p)?;
    let (program, title) = (text(p, "program")?, text(p, "title")?);
    Ok(change(lib, &path, |i| {
        if let Some(program) = program {
            i.program = clean_source(&program);
        }
        if let Some(title) = title {
            i.title = clean_source(&title);
        }
    }))
}

fn forget(lib: &mut Library, p: &Value) -> Result<Value> {
    let path = path(p)?;
    lib.set(&path, CaptureInfo::default());
    Ok(answer(lib, &path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_favourites_and_sources_go_through_commands() {
        let mut lib = Library::default();
        let p = "C:/Pictures/ScreenGrabCraft/Capture 1.png";
        run(&mut lib, "library.tag", &json!({"path": p, "tag": " receipts "})).unwrap();
        run(&mut lib, "library.tag", &json!({"path": p, "tag": "Receipts"})).unwrap();
        let v = run(&mut lib, "library.favorite", &json!({"path": p})).unwrap();
        assert_eq!(v["favorite"], json!(true));
        assert_eq!(v["tags"], json!(["receipts"]));
        run(&mut lib, "library.source", &json!({"path": p, "program": "Notes", "title": "List\n"})).unwrap();
        assert_eq!(lib.get(p).title, "List");
        assert_eq!(run(&mut lib, "library.favorite", &json!({"path": p, "on": true})).unwrap()["favorite"], json!(true));
        run(&mut lib, "library.untag", &json!({"path": p, "tag": "RECEIPTS"})).unwrap();
        assert!(lib.get(p).tags.is_empty());
        run(&mut lib, "library.forget", &json!({"path": p})).unwrap();
        assert!(lib.captures.is_empty());
    }

    #[test]
    fn hostile_params_never_panic_or_change_the_library() {
        let junk = [
            json!(null),
            json!(1),
            json!("x"),
            json!([]),
            json!({}),
            json!({"path": 5, "tag": "a"}),
            json!({"path": "", "tag": "a"}),
            json!({"path": "a.png", "tag": "   "}),
            json!({"path": "a.png", "tag": ["x"], "on": "yes", "program": 7}),
            json!({"path": "x".repeat(10_000), "tag": "é".repeat(50_000)}),
        ];
        for c in LIBRARY_COMMANDS {
            for p in &junk {
                let mut lib = Library::default();
                lib.set("a.png", CaptureInfo { tags: vec!["kept".into()], ..CaptureInfo::default() });
                let before = lib.clone();
                match run(&mut lib, c.id, p) {
                    Ok(_) => {}
                    Err(EngineError::Panicked(id)) => panic!("{id} panicked on {p}"),
                    Err(_) => assert_eq!(lib, before, "{} with {p} failed but changed the library", c.id),
                }
            }
        }
        assert!(matches!(run(&mut Library::default(), "library.teapot", &json!({})), Err(EngineError::Unknown(_))));
    }

    #[test]
    fn every_library_command_is_unique_and_described() {
        let mut ids: Vec<&str> = LIBRARY_COMMANDS.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n);
        assert!(LIBRARY_COMMANDS.iter().all(|c| !c.label.is_empty() && !c.params.is_empty() && c.id.starts_with("library.")));
        assert_eq!(describe().as_array().map(Vec::len), Some(n));
    }
}
