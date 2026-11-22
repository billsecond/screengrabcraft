//! Batch changes: the same commands run on many pictures, one after another, each in a session of
//! its own. The host reads and writes the files; this only changes pictures.

use screengrabcraft_model::{Document, Raster};
use serde_json::Value;

use crate::session::Session;
use crate::{EngineError, Result};

/// The most changes one batch makes to a picture.
pub const MAX_STEPS: usize = 64;

/// The picture with each command (`id`, its parameters) run on it in turn, and drawn with whatever
/// annotations they added. The first that fails stops it, saying which.
pub fn apply(picture: Raster, steps: &[(String, Value)]) -> Result<Raster> {
    if steps.len() > MAX_STEPS {
        return Err(EngineError::Params(format!("at most {MAX_STEPS} changes at once")));
    }
    let mut s = Session::new(Document::new(picture));
    for (id, params) in steps {
        if id.starts_with("edit.") {
            return Err(EngineError::Params(format!("`{id}` is no change to a picture")));
        }
        s.execute(id, params).map_err(|e| EngineError::Failed(format!("{id}: {e}")))?;
    }
    screengrabcraft_render::render(&s.doc).ok_or_else(|| EngineError::Failed("the picture could not be drawn".to_string()))
}

#[cfg(test)]
mod tests {
    use screengrabcraft_model::Color;
    use serde_json::json;

    use super::*;

    fn steps(list: &[(&str, Value)]) -> Vec<(String, Value)> {
        list.iter().map(|(id, p)| (id.to_string(), p.clone())).collect()
    }

    #[test]
    fn every_change_is_made_in_turn() {
        let pic = Raster::filled(200, 100, Color::rgb(200, 30, 30)).unwrap();
        let out =
            apply(pic, &steps(&[("image.resize", json!({"percent": 50})), ("effect.border", json!({"width": 5})), ("effect.grayscale", json!({}))]))
                .unwrap();
        assert_eq!((out.width(), out.height()), (110, 60), "half the size, and a border round it");
        let c = out.pixel(50, 30).unwrap();
        assert!(c.r == c.g && c.g == c.b, "grey: {c:?}");
    }

    #[test]
    fn a_change_that_fails_says_which_and_nonsense_never_panics() {
        let pic = || Raster::filled(40, 30, Color::WHITE).unwrap();
        let err = apply(pic(), &steps(&[("effect.grayscale", json!({})), ("image.resize", json!({"percent": -5}))])).unwrap_err();
        assert!(err.to_string().contains("image.resize"), "{err}");
        for junk in [json!(null), json!([1, 2]), json!({"width": "wide", "degrees": 1e300})] {
            for c in crate::COMMANDS {
                let _ = apply(pic(), &[(c.id.to_string(), junk.clone())]);
            }
        }
        assert!(apply(pic(), &steps(&[("edit.undo", json!({}))])).is_err(), "not a change");
        assert!(apply(pic(), &vec![("effect.invert".to_string(), json!({})); MAX_STEPS + 1]).is_err());
        assert_eq!(apply(pic(), &[]).map(|r| (r.width(), r.height())).ok(), Some((40, 30)), "no changes: as it was");
    }
}
