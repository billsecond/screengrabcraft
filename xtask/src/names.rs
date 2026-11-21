//! `cargo xtask names`: no file in the repository names the commercial capture and video tools
//! ScreenGrabCraft is modelled on, or their maker. A feature is described by what it does.
//!
//! The words aren't written here either: the check keeps each one's length and FNV-1a hash, and
//! hashes every run of that many bytes (lower-cased) in every text file and every file's path, so
//! it finds a word in any case and inside another (`Word's`, `Word-like`, `WordStyle`).
//! `cargo xtask names hash <word>` prints the entry for another word.

use std::path::Path;
use std::process::Command;

/// A word that must not appear: its length in bytes and the FNV-1a hash of it, lower-cased.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Banned {
    len: usize,
    hash: u64,
}

/// The maker and its two products.
const BANNED: &[Banned] =
    &[Banned { len: 9, hash: 0xd464_33a6_74ca_1b94 }, Banned { len: 6, hash: 0x9e6a_e9c5_dc81_b245 }, Banned { len: 8, hash: 0x8856_7b20_6bc1_1336 }];

/// Files bigger than this aren't read: none of the text is near it, and the big ones are models.
const MAX_TEXT: u64 = 16 * 1024 * 1024;

/// FNV-1a, 64 bits, of the bytes lower-cased (ASCII).
pub fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b.to_ascii_lowercase())).wrapping_mul(0x0000_0100_0000_01b3))
}

/// The entry for `word`.
pub fn entry(word: &str) -> Banned {
    Banned { len: word.len(), hash: fnv1a(word.as_bytes()) }
}

/// The lines of `text` (counted from 1) on which one of `banned` starts.
pub fn lines_naming(text: &str, banned: &[Banned]) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    for word in banned.iter().filter(|w| w.len > 0) {
        for (at, run) in bytes.windows(word.len).enumerate() {
            if fnv1a(run) == word.hash {
                let before = bytes.get(..at).unwrap_or_default();
                lines.push(before.iter().filter(|&&c| c == b'\n').count() + 1);
            }
        }
    }
    lines.sort_unstable();
    lines.dedup();
    lines
}

pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    match args {
        [] => check(root),
        ["hash", word] => {
            let e = entry(word);
            println!("    Banned {{ len: {}, hash: {:#018x} }},", e.len, e.hash);
            Ok(())
        }
        _ => Err("usage: cargo xtask names [hash <word>]".into()),
    }
}

fn check(root: &Path) -> Result<(), String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .output()
        .map_err(|e| format!("git ls-files: {e}"))?;
    if !out.status.success() {
        return Err(format!("git ls-files failed:\n{}", String::from_utf8_lossy(&out.stderr)));
    }
    let mut found = Vec::new();
    let mut read = 0usize;
    for path in String::from_utf8_lossy(&out.stdout).lines() {
        if !lines_naming(path, BANNED).is_empty() {
            found.push(format!("{path} (the file's name)"));
        }
        let full = root.join(path);
        let Ok(meta) = std::fs::metadata(&full) else { continue };
        if !meta.is_file() || meta.len() > MAX_TEXT {
            continue;
        }
        let Ok(bytes) = std::fs::read(&full) else { continue };
        // Not text: a picture, a font, a model.
        let Ok(text) = String::from_utf8(bytes) else { continue };
        read += 1;
        found.extend(lines_naming(&text, BANNED).into_iter().map(|line| format!("{path}:{line}")));
    }
    if found.is_empty() {
        println!("names: none of {read} text files names another capture or video product or its maker");
        Ok(())
    } else {
        Err(format!(
            "{} place(s) name another capture or video product or its maker; say what the feature does instead:\n  {}",
            found.len(),
            found.join("\n  ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_fnv1a_and_ignores_case() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(b"WiDgEt"), fnv1a(b"widget"));
    }

    #[test]
    fn a_word_is_found_in_any_case_and_inside_others() {
        let banned = [entry("widget")];
        let text = "first line\nA Widget-like tool\nnothing here\nmy_WIDGETS and widget again\n";
        assert_eq!(lines_naming(text, &banned), vec![2, 4]);
        assert!(lines_naming("wid get, widge", &banned).is_empty());
        assert!(lines_naming("", &banned).is_empty());
        assert!(lines_naming("widget", &[Banned { len: 0, hash: 0 }]).is_empty());
    }

    #[test]
    fn every_entry_is_a_whole_word() {
        assert!(!BANNED.is_empty());
        assert!(BANNED.iter().all(|w| w.len >= 5));
    }
}
