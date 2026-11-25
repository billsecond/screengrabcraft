//! `cargo xtask shortcuts`: the Start menu entries the installers make, what each one starts, and
//! that typing "record" or "edit" into the Start menu's search brings them up.
//!
//! Windows' Start search finds a desktop program by the start of any word of its shortcut's name
//! ("edit" finds "Registry Editor"). It doesn't read the descriptions of this installer's
//! shortcuts (they are advertised ones, whose description Windows Search leaves out), so the names
//! carry the words. Two shortcuts of one installed program are one app to Start, which then lists
//! only one of them, unless each has an app ID of its own (`System.AppUserModel.ID`).
//!
//! Linux desktops search an entry's name, generic name, keywords and comment; KDE also lists its
//! actions (the Video Editor, Record), which every desktop offers on the entry's right-click.

use std::path::Path;

const WXS: &str = "packaging/windows/screengrabcraft.wxs";
const SERVICE_DESKTOP: &str = "packaging/linux/io.github.billsecond.screengrabcraft.desktop";
const EDITOR_DESKTOP: &str = "packaging/linux/io.github.billsecond.screengrabcraft.editor.desktop";

/// What starts a new recording: the Recorder, as the tray's red-dot item opens it.
const RECORD: &str = "--capture production";
/// What opens the Video Editor (the editor's own switch).
const VIDEO: &str = "--video";

/// Every Start menu shortcut the Windows installer makes: its name, the program it starts, that
/// program's arguments, and its own app ID (none: the program's).
const WINDOWS: &[(&str, &str, &str, Option<&str>)] = &[
    ("ScreenGrabCraft", "screengrabcraft.exe", "", None),
    ("ScreenGrabCraft Recorder", "screengrabcraft.exe", RECORD, Some("io.github.billsecond.screengrabcraft.recorder")),
    ("ScreenGrabCraft Image Editor", "screengrabcraft-editor.exe", "", None),
    ("ScreenGrabCraft Video Editor", "screengrabcraft-editor.exe", VIDEO, Some("io.github.billsecond.screengrabcraft.video")),
];

/// What people type into the Start menu, and the shortcuts of ours it must bring up (and no others).
const SEARCHES: &[(&str, &[&str])] =
    &[("record", &["ScreenGrabCraft Recorder"]), ("edit", &["ScreenGrabCraft Image Editor", "ScreenGrabCraft Video Editor"])];

/// A Linux desktop entry: its file, its command, the words it must be found by, and its actions
/// (id, name, program, arguments, a word it must be found by).
struct Entry {
    file: &'static str,
    exec: &'static str,
    words: &'static [&'static str],
    actions: &'static [(&'static str, &'static str, &'static str, &'static str, &'static str)],
}

const LINUX: &[Entry] = &[
    Entry {
        file: SERVICE_DESKTOP,
        exec: "screengrabcraft",
        words: &["record"],
        actions: &[("record", "Record", "screengrabcraft", RECORD, "record")],
    },
    Entry {
        file: EDITOR_DESKTOP,
        exec: "screengrabcraft-editor %F",
        words: &["edit"],
        actions: &[("video", "Video Editor", "screengrabcraft-editor", VIDEO, "edit")],
    },
];

/// A Start menu shortcut in the Windows installer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub name: String,
    /// The installed program it starts (its file name).
    pub exe: String,
    pub args: String,
    pub directory: String,
    pub app_id: Option<String>,
}

pub fn run(root: &Path) -> Result<(), String> {
    let read = |p: &str| std::fs::read_to_string(root.join(p)).map_err(|e| format!("{p}: {e}"));
    let wxs = read(WXS)?;
    let mut problems = check_windows(&wxs);
    for e in LINUX {
        problems.extend(check_linux(e, &read(e.file)?));
    }
    if problems.is_empty() {
        let names: Vec<String> = windows_shortcuts(&wxs).iter().map(|s| format!("\"{}\"", s.name)).collect();
        println!("shortcuts: the Start menu has {}; \"record\" and \"edit\" find theirs; the Linux entries match", names.join(", "));
        Ok(())
    } else {
        Err(format!("{} problem(s) with the Start menu entries:\n  {}", problems.len(), problems.join("\n  ")))
    }
}

/// Whether a search for `typed` finds `text`: one of its words starts with it, in any case.
pub fn finds(text: &str, typed: &str) -> bool {
    let typed = typed.to_lowercase();
    !typed.is_empty() && text.split(|c: char| !c.is_alphanumeric()).any(|w| w.to_lowercase().starts_with(&typed))
}

pub fn check_windows(wxs: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let xml = without_comments(wxs);
    let found = windows_shortcuts(wxs);
    if elements(&xml, "Shortcut").len() != found.len() {
        problems.push(format!("{WXS}: a shortcut that isn't on its program's <File> isn't checked here; put it on the File, as the others"));
    }
    for (name, exe, args, app_id) in WINDOWS {
        let same: Vec<&Shortcut> = found.iter().filter(|s| s.name == *name).collect();
        let [s] = same.as_slice() else {
            problems.push(format!("the installer has {} Start menu shortcuts named \"{name}\", not one", same.len()));
            continue;
        };
        if s.exe != *exe {
            problems.push(format!("\"{name}\" starts {}, not {exe}", s.exe));
        }
        if s.args != *args {
            problems.push(format!("\"{name}\" starts {exe} with \"{}\", not \"{args}\"", s.args));
        }
        if s.directory != "ProgramMenuFolder" {
            problems.push(format!("\"{name}\" goes in {}, not the Start menu (ProgramMenuFolder)", s.directory));
        }
        if s.app_id.as_deref() != *app_id {
            problems.push(format!("\"{name}\" has the app ID {:?}, not {app_id:?}", s.app_id));
        }
    }
    for s in &found {
        if !WINDOWS.iter().any(|w| w.0 == s.name) {
            problems.push(format!("Start menu shortcut \"{}\" isn't in xtask/src/shortcuts.rs: add what it starts there", s.name));
        }
    }
    // One app to Start per program, unless a shortcut has an app ID of its own.
    for (i, s) in found.iter().enumerate() {
        let one_app = |o: &Shortcut| match (&s.app_id, &o.app_id) {
            (Some(a), Some(b)) => a == b,
            (None, None) => s.exe == o.exe,
            _ => false,
        };
        if let Some(o) = found.iter().skip(i + 1).find(|o| one_app(o)) {
            problems
                .push(format!("\"{}\" and \"{}\" are one app to Start, which lists only one of them: give one an app ID of its own", s.name, o.name));
        }
    }
    for (typed, wanted) in SEARCHES {
        let mut got: Vec<&str> = found.iter().filter(|s| finds(&s.name, typed)).map(|s| s.name.as_str()).collect();
        let mut wanted = wanted.to_vec();
        got.sort_unstable();
        wanted.sort_unstable();
        if got != wanted {
            problems.push(format!("typing \"{typed}\" in Start brings up {got:?}, not {wanted:?}"));
        }
    }
    // The sign-in entry stays as it is.
    let sign_in = elements(&xml, "RegistryValue").into_iter().any(|(t, _)| {
        attr(t, "Root").as_deref() == Some("HKMU")
            && attr(t, "Key").as_deref() == Some(r"Software\Microsoft\Windows\CurrentVersion\Run")
            && attr(t, "Name").as_deref() == Some("ScreenGrabCraft")
            && attr(t, "Value").as_deref() == Some("\"[INSTALLFOLDER]screengrabcraft.exe\" --background")
    });
    if !sign_in {
        problems.push(
            r#"the sign-in entry (HKMU Run, ScreenGrabCraft = "[INSTALLFOLDER]screengrabcraft.exe" --background) is missing or changed"#.to_string(),
        );
    }
    problems
}

/// The Start menu shortcuts in the installer's source: each `<Shortcut>` on a program's `<File>`.
pub fn windows_shortcuts(wxs: &str) -> Vec<Shortcut> {
    let xml = without_comments(wxs);
    let mut out = Vec::new();
    for (file, inside) in elements(&xml, "File") {
        let source = attr(file, "Source").unwrap_or_default();
        let exe = source.rsplit(['\\', '/']).next().unwrap_or_default().to_string();
        for (tag, props) in elements(inside, "Shortcut") {
            let app_id = elements(props, "ShortcutProperty")
                .into_iter()
                .find(|(p, _)| attr(p, "Key").as_deref() == Some("System.AppUserModel.ID"))
                .and_then(|(p, _)| attr(p, "Value"));
            out.push(Shortcut {
                name: attr(tag, "Name").unwrap_or_default(),
                exe: exe.clone(),
                args: attr(tag, "Arguments").unwrap_or_default(),
                directory: attr(tag, "Directory").unwrap_or_default(),
                app_id,
            });
        }
    }
    out
}

fn check_linux(e: &Entry, text: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let groups = desktop_groups(text);
    let get = |group: &str, key: &str| value(&groups, group, key);
    let main = "Desktop Entry";
    if get(main, "Exec") != Some(e.exec) {
        problems.push(format!("{}: Exec is {:?}, not {:?}", e.file, get(main, "Exec"), e.exec));
    }
    let searched: Vec<&str> = ["Name", "GenericName", "Comment"].iter().filter_map(|k| get(main, k)).collect();
    let keywords: Vec<&str> = get(main, "Keywords").unwrap_or_default().split(';').filter(|k| !k.is_empty()).collect();
    for typed in e.words {
        if !searched.iter().chain(&keywords).any(|t| finds(t, typed)) {
            problems.push(format!("{}: typing \"{typed}\" doesn't find it (its name, generic name, comment or keywords)", e.file));
        }
    }
    let listed: Vec<&str> = get(main, "Actions").unwrap_or_default().split(';').filter(|a| !a.is_empty()).collect();
    for (id, name, program, args, typed) in e.actions {
        let group = format!("Desktop Action {id}");
        let exec = format!("{program} {args}");
        if !listed.contains(id) {
            problems.push(format!("{}: Actions doesn't list \"{id}\"", e.file));
        }
        if get(&group, "Name") != Some(*name) {
            problems.push(format!("{}: [{group}] is named {:?}, not {name:?}", e.file, get(&group, "Name")));
        }
        if get(&group, "Exec") != Some(exec.as_str()) {
            problems.push(format!("{}: [{group}] runs {:?}, not {exec:?}", e.file, get(&group, "Exec")));
        }
        if !finds(name, typed) {
            problems.push(format!("{}: typing \"{typed}\" doesn't find the action \"{name}\"", e.file));
        }
    }
    problems
}

type Groups = Vec<(String, Vec<(String, String)>)>;

/// A key's value in a desktop entry's group.
fn value<'a>(groups: &'a Groups, group: &str, key: &str) -> Option<&'a str> {
    groups.iter().find(|(g, _)| g == group).and_then(|(_, keys)| keys.iter().find(|(k, _)| k == key)).map(|(_, v)| v.as_str())
}

/// A desktop entry's groups, each with its keys and values, in order.
fn desktop_groups(text: &str) -> Groups {
    let mut groups = Groups::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            groups.push((name.to_string(), Vec::new()));
        } else if let (Some((k, v)), Some((_, keys))) = (line.split_once('='), groups.last_mut()) {
            keys.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    groups
}

/// The XML without its comments.
fn without_comments(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(start) = rest.find("<!--") {
        out.push_str(rest.get(..start).unwrap_or_default());
        rest = rest.get(start..).and_then(|r| r.find("-->").and_then(|end| r.get(end + 3..))).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

/// Every `<name …>` element: its opening tag, and what is inside it (nothing for `<name … />`). The
/// files checked here never put an element inside one of its own kind.
fn elements<'a>(xml: &'a str, name: &str) -> Vec<(&'a str, &'a str)> {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find(&open) {
        let after = rest.get(at + open.len()..).unwrap_or_default();
        if !after.starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/') {
            rest = after;
            continue;
        }
        let Some(end) = after.find('>') else { break };
        let tag = after.get(..end).unwrap_or_default();
        let body = after.get(end + 1..).unwrap_or_default();
        if tag.ends_with('/') {
            out.push((tag, ""));
            rest = body;
        } else {
            let inside_end = body.find(&close).unwrap_or(body.len());
            out.push((tag, body.get(..inside_end).unwrap_or_default()));
            rest = body.get(inside_end..).unwrap_or_default();
        }
    }
    out
}

/// An attribute of an opening tag, with the XML escapes undone.
fn attr(tag: &str, key: &str) -> Option<String> {
    let pattern = format!("{key}=\"");
    let mut from = 0;
    while let Some(i) = tag.get(from..).and_then(|t| t.find(&pattern)).map(|i| i + from) {
        let value = tag.get(i + pattern.len()..).unwrap_or_default();
        if tag.get(..i).and_then(|b| b.chars().last()).is_some_and(char::is_whitespace) {
            let v = value.get(..value.find('"')?)?;
            return Some(v.replace("&quot;", "\"").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&"));
        }
        from = i + pattern.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(path: &str) -> String {
        std::fs::read_to_string(crate::root().join(path)).unwrap()
    }

    #[test]
    fn the_installers_start_menu_is_as_listed() {
        assert_eq!(check_windows(&repo(WXS)), Vec::<String>::new());
        for e in LINUX {
            assert_eq!(check_linux(e, &repo(e.file)), Vec::<String>::new(), "{}", e.file);
        }
    }

    #[test]
    fn record_and_edit_find_the_recorder_and_both_editors() {
        let names: Vec<String> = windows_shortcuts(&repo(WXS)).into_iter().map(|s| s.name).collect();
        let found = |typed: &str| names.iter().filter(|n| finds(n, typed)).cloned().collect::<Vec<_>>();
        assert_eq!(found("record"), ["ScreenGrabCraft Recorder"]);
        assert_eq!(found("Edit"), ["ScreenGrabCraft Image Editor", "ScreenGrabCraft Video Editor"]);
        assert_eq!(found("screengrab").len(), 4);
        assert!(found("").is_empty());
    }

    #[test]
    fn words_are_found_by_their_start_only() {
        assert!(finds("Registry Editor", "edit"));
        assert!(finds("ScreenGrabCraft Recorder", "RECORD"));
        assert!(!finds("ScreenGrabCraft Recorder", "corder"));
        assert!(!finds("gedit", "edit"));
    }

    #[test]
    fn a_shortcut_that_starts_the_wrong_thing_or_hides_another_is_caught() {
        let wxs = repo(WXS);
        // The Recorder made to start the editor's video switch.
        let wrong = wxs.replace(r#"Arguments="--capture production""#, r#"Arguments="--video""#);
        assert!(check_windows(&wrong).iter().any(|p| p.contains("ScreenGrabCraft Recorder") && p.contains("--video")));
        // Without its own app ID, Start would list only one of it and ScreenGrabCraft.
        let merged = wxs.replace(r#"Value="io.github.billsecond.screengrabcraft.recorder""#, r#"Value="io.github.billsecond.screengrabcraft.video""#);
        assert!(check_windows(&merged).iter().any(|p| p.contains("one app to Start")));
        // Gone.
        let gone = wxs.replace(r#"Name="ScreenGrabCraft Recorder""#, r#"Name="ScreenGrabCraft Capture""#);
        let problems = check_windows(&gone);
        assert!(problems.iter().any(|p| p.contains("0 Start menu shortcuts named \"ScreenGrabCraft Recorder\"")));
        assert!(problems.iter().any(|p| p.contains("typing \"record\"")));
        // The sign-in entry changed.
        let no_sign_in = wxs.replace("--background\" Type=\"string\" KeyPath", "--tray\" Type=\"string\" KeyPath");
        assert!(check_windows(&no_sign_in).iter().any(|p| p.contains("sign-in entry")));
    }

    #[test]
    fn a_linux_entry_without_its_action_is_caught() {
        let service = &LINUX[0];
        let text = repo(service.file).replace("Exec=screengrabcraft --capture production", "Exec=screengrabcraft");
        assert!(check_linux(service, &text).iter().any(|p| p.contains("[Desktop Action record] runs")));
        let text = repo(service.file).replace("Actions=record;", "");
        assert!(check_linux(service, &text).iter().any(|p| p.contains("Actions doesn't list")));
    }

    #[test]
    fn the_xml_reader_reads_what_the_installer_writes() {
        let xml = r#"<File Id="A" Source="$(var.BinDir)\a.exe"><!-- <Shortcut Name="Not this" /> -->
            <Shortcut Id="S" Directory="ProgramMenuFolder" Name="A &amp; B" Arguments="--x &quot;y&quot;" />
            <Shortcut Id="T" Directory="ProgramMenuFolder" Name="T"><ShortcutProperty Key="System.AppUserModel.ID" Value="t" /></Shortcut>
            </File><FileSearch Name="no" />"#;
        let s = windows_shortcuts(xml);
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].name.as_str(), s[0].exe.as_str(), s[0].args.as_str(), s[0].app_id.as_deref()), ("A & B", "a.exe", "--x \"y\"", None));
        assert_eq!((s[1].name.as_str(), s[1].app_id.as_deref()), ("T", Some("t")));
        // Broken input: nothing, no panic.
        for bad in ["<File", "<File Source=\"x", "<!-- open", "<File><Shortcut Name=\"x\"", "<Shortcut Name=\"\u{e9}\"/>"] {
            let _ = windows_shortcuts(bad);
            let _ = check_windows(bad);
        }
        assert_eq!(attr(r#" xName="no" Name="yes""#, "Name").as_deref(), Some("yes"));
    }
}
