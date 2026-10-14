//! What `--version` and About say.

/// The version (the packaging may override it).
pub fn version() -> &'static str {
    option_env!("SCREENGRABCRAFT_VERSION").filter(|v| !v.is_empty()).unwrap_or(screengrabcraft_model::VERSION)
}

/// The commit a release was built from, shortened.
pub fn commit() -> Option<&'static str> {
    option_env!("SCREENGRABCRAFT_BUILD_SHA").filter(|s| !s.is_empty()).map(|s| s.get(..9).unwrap_or(s))
}

pub fn date() -> Option<&'static str> {
    option_env!("SCREENGRABCRAFT_BUILD_DATE").filter(|s| !s.is_empty())
}

/// `ScreenGrabCraft 0.1.0 (commit 1a2b3c4d5, 2026-10-14)`.
pub fn line(program: &str) -> String {
    match (commit(), date()) {
        (Some(c), Some(d)) => format!("{program} {} (commit {c}, {d})", version()),
        (Some(c), None) => format!("{program} {} (commit {c})", version()),
        _ => format!("{program} {}", version()),
    }
}
