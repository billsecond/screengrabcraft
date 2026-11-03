//! One running copy of each program.
//!
//! The first copy takes a lock in the settings folder and listens on a loopback port. A copy
//! started later (a shortcut clicked again, `screengrabcraft --capture region` from a desktop
//! shortcut, a file opened in the editor from the file manager) finds the lock taken, hands its
//! arguments to the first, and exits. The other program can reach it the same way ([`tell`]).
//!
//! - `<name>.lock`: held locked while the first copy runs (the system lets go of it however the
//!   program ends, so a crash leaves nothing to clean up).
//! - `<name>.instance`: the port and a random key, which every hand-over must quote, so another
//!   program on the machine can't drive this one (the settings folder is the owner's own).
//!
//! Runs without settings take no lock.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const MAX_LINE: usize = 1 << 20;
const MAX_ARGS: usize = 256;
const MAX_ARG: usize = 32 * 1024;
const ANSWER: Duration = Duration::from_secs(5);
/// How long a later copy keeps trying to reach the first (which may itself be starting).
const REACH: Duration = Duration::from_secs(8);

/// Called when something is handed over, so the program wakes up.
pub type Waker = Arc<Mutex<Option<Box<dyn Fn() + Send>>>>;

pub enum Start {
    /// This is the first copy; keep the guard for as long as it runs.
    First(Guard),
    /// Another copy runs and took the arguments: exit.
    Handed,
    /// Another copy runs but couldn't be reached.
    Unreachable(String),
}

pub struct Guard {
    _lock: Option<File>,
    info: Option<PathBuf>,
    rx: Receiver<Vec<String>>,
    waker: Waker,
}

impl Guard {
    /// A guard that guards nothing (runs without settings).
    pub fn none() -> Guard {
        let (_tx, rx) = channel();
        Guard { _lock: None, info: None, rx, waker: Arc::new(Mutex::new(None)) }
    }

    /// What to call when something arrives.
    pub fn set_waker(&self, f: Box<dyn Fn() + Send>) {
        *self.waker.lock().unwrap_or_else(PoisonError::into_inner) = Some(f);
    }

    /// Everything handed over since the last look, one list of arguments per later start.
    pub fn handed(&self) -> Vec<Vec<String>> {
        let mut out = Vec::new();
        while let Ok(args) = self.rx.try_recv() {
            out.push(args);
        }
        out
    }

    /// The program is ending: later starts have nobody to reach.
    pub fn close(&self) {
        if let Some(info) = &self.info {
            let _ = std::fs::remove_file(info);
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.close();
    }
}

fn random_key() -> String {
    let mut bytes = [0u8; 24];
    if getrandom::fill(&mut bytes).is_err() {
        // A key nobody can read is still better than none; the settings folder is private.
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        bytes.iter_mut().enumerate().for_each(|(i, b)| *b = (t >> (i % 16 * 8)) as u8 ^ (i as u8).wrapping_mul(97));
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Compares in constant time.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Starts program `name`: takes the lock, or hands `args` to the copy that has it.
pub fn start(name: &str, args: &[String]) -> Start {
    if crate::paths::no_prefs() {
        return Start::First(Guard::none());
    }
    let Some(dir) = crate::paths::ensure_settings_dir() else { return Start::First(Guard::none()) };
    let lock_path = dir.join(format!("{name}.lock"));
    let info_path = dir.join(format!("{name}.instance"));
    let lock = match OpenOptions::new().create(true).truncate(false).read(true).write(true).open(&lock_path) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("{}: {e}; running without the one-copy guard", lock_path.display());
            return Start::First(Guard::none());
        }
    };
    match lock.try_lock() {
        Ok(()) => first(lock, info_path),
        Err(TryLockError::WouldBlock) => hand_over(&info_path, args),
        Err(TryLockError::Error(e)) => {
            log::warn!("locking {}: {e}; running without the one-copy guard", lock_path.display());
            Start::First(Guard::none())
        }
    }
}

/// Tells the running copy of program `name` something, without starting one: `true` if it took
/// it. May take seconds if that copy is stuck, so not on a window's thread.
pub fn tell(name: &str, args: &[String]) -> bool {
    if crate::paths::no_prefs() {
        return false;
    }
    crate::paths::settings_dir().is_some_and(|dir| tell_in(&dir, name, args))
}

/// Whether a copy of program `name` is running.
pub fn running(name: &str) -> bool {
    !crate::paths::no_prefs() && crate::paths::settings_dir().is_some_and(|dir| running_in(&dir, name))
}

fn running_in(dir: &std::path::Path, name: &str) -> bool {
    if !dir.join(format!("{name}.instance")).exists() {
        return false;
    }
    // A crash leaves the info behind, and Windows takes seconds to give up on its dead port, so
    // the lock says whether anyone is there.
    let Ok(lock) = OpenOptions::new().read(true).write(true).open(dir.join(format!("{name}.lock"))) else { return false };
    matches!(lock.try_lock(), Err(TryLockError::WouldBlock))
}

fn tell_in(dir: &std::path::Path, name: &str, args: &[String]) -> bool {
    running_in(dir, name) && try_hand(&dir.join(format!("{name}.instance")), args).is_ok()
}

fn first(lock: File, info_path: PathBuf) -> Start {
    let listener = match TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("no loopback port for later starts: {e}");
            let (_tx, rx) = channel();
            return Start::First(Guard { _lock: Some(lock), info: None, rx, waker: Arc::new(Mutex::new(None)) });
        }
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let key = random_key();
    let info = json!({ "port": port, "key": key, "pid": std::process::id() }).to_string();
    if let Err(e) = crate::store::write_atomic(&info_path, info.as_bytes()) {
        log::warn!("{}: {e}", info_path.display());
    }
    restrict(&info_path);
    let (tx, rx) = channel();
    let waker: Waker = Arc::new(Mutex::new(None));
    let w = waker.clone();
    let spawned = std::thread::Builder::new().name("instance".into()).spawn(move || serve(listener, key, tx, w));
    if let Err(e) = spawned {
        log::warn!("no listener for later starts: {e}");
    }
    Start::First(Guard { _lock: Some(lock), info: Some(info_path), rx, waker })
}

#[cfg(unix)]
fn restrict(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict(_path: &std::path::Path) {}

fn serve(listener: TcpListener, key: String, tx: Sender<Vec<String>>, waker: Waker) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let _ = stream.set_read_timeout(Some(ANSWER));
        let _ = stream.set_write_timeout(Some(ANSWER));
        if let Some(args) = read_request(&stream, &key) {
            let mut s = &stream;
            let _ = s.write_all(b"{\"ok\":true}\n");
            if tx.send(args).is_err() {
                return;
            }
            if let Some(f) = waker.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
                f();
            }
        } else {
            let mut s = &stream;
            let _ = s.write_all(b"{\"ok\":false}\n");
        }
    }
}

fn read_request(stream: &TcpStream, key: &str) -> Option<Vec<String>> {
    let mut line = String::new();
    BufReader::new(stream.take(MAX_LINE as u64)).read_line(&mut line).ok()?;
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    if !same(v.get("key")?.as_str()?, key) {
        log::warn!("a hand-over with the wrong key was refused");
        return None;
    }
    let args = v.get("args")?.as_array()?;
    Some(args.iter().take(MAX_ARGS).filter_map(Value::as_str).map(|a| a.chars().take(MAX_ARG).collect()).collect())
}

fn hand_over(info_path: &std::path::Path, args: &[String]) -> Start {
    let deadline = Instant::now() + REACH;
    let mut last = String::from("the running copy didn't answer");
    while Instant::now() < deadline {
        match try_hand(info_path, args) {
            Ok(()) => return Start::Handed,
            Err(e) => last = e,
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Start::Unreachable(last)
}

fn try_hand(info_path: &std::path::Path, args: &[String]) -> Result<(), String> {
    let text = std::fs::read_to_string(info_path).map_err(|e| format!("{}: {e}", info_path.display()))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let port = v.get("port").and_then(Value::as_u64).and_then(|p| u16::try_from(p).ok()).ok_or("no port")?;
    let key = v.get("key").and_then(Value::as_str).ok_or("no key")?;
    let mut stream = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), ANSWER).map_err(|e| e.to_string())?;
    let _ = stream.set_read_timeout(Some(ANSWER));
    let msg = json!({ "key": key, "args": args }).to_string() + "\n";
    stream.write_all(msg.as_bytes()).map_err(|e| e.to_string())?;
    let mut answer = String::new();
    BufReader::new(&stream).read_line(&mut answer).map_err(|e| e.to_string())?;
    if answer.contains("\"ok\":true") { Ok(()) } else { Err("the running copy refused the hand-over".to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_compare_and_requests_are_checked() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "ab"));
        let k = random_key();
        assert_eq!(k.len(), 48);
        assert_ne!(k, random_key());
    }

    #[test]
    fn a_hand_over_reaches_the_listener() {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = channel();
        let waker: Waker = Arc::new(Mutex::new(None));
        std::thread::spawn(move || serve(listener, "secret".into(), tx, waker));
        let dir = std::env::temp_dir().join(format!("sgc-instance-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let info = dir.join("t.instance");
        std::fs::write(&info, json!({"port": port, "key": "secret"}).to_string()).unwrap();
        try_hand(&info, &["--capture".to_string(), "region".to_string()]).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), vec!["--capture", "region"]);
        std::fs::write(&info, json!({"port": port, "key": "wrong"}).to_string()).unwrap();
        assert!(try_hand(&info, &[]).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn telling_reaches_a_running_copy_and_nobody_else() {
        let dir = std::env::temp_dir().join(format!("sgc-instance-tell-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (lock, info) = (dir.join("t.lock"), dir.join("t.instance"));
        let word = ["--word".to_string()];
        assert!(!tell_in(&dir, "t", &word), "nothing there");

        // What a crash leaves: the info of a port nobody listens on, and a free lock.
        std::fs::write(&info, json!({"port": 1, "key": "secret"}).to_string()).unwrap();
        std::fs::write(&lock, "").unwrap();
        let asked = Instant::now();
        assert!(!tell_in(&dir, "t", &word));
        assert!(asked.elapsed() < Duration::from_millis(500), "the dead port isn't tried");
        assert!(!running_in(&dir, "t"));

        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = channel();
        let waker: Waker = Arc::new(Mutex::new(None));
        std::thread::spawn(move || serve(listener, "secret".into(), tx, waker));
        std::fs::write(&info, json!({"port": port, "key": "secret"}).to_string()).unwrap();
        let held = OpenOptions::new().read(true).write(true).open(&lock).unwrap();
        held.try_lock().unwrap();
        assert!(running_in(&dir, "t"));
        assert!(tell_in(&dir, "t", &word));
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), word);
        drop(held);
        assert!(!running_in(&dir, "t"), "the copy has gone");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
