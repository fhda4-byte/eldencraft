//! Plain text log at %LOCALAPPDATA%\EldenCraft\eldencraft.log (the oracle for every in-game test).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static FILE: Mutex<Option<File>> = Mutex::new(None);

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("EldenCraft")
}

pub fn init() {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("eldencraft.log");
    // Keep the previous run's log for comparison.
    let _ = std::fs::rename(&path, dir.join("eldencraft.prev.log"));
    if let Ok(f) = OpenOptions::new().create(true).write(true).truncate(true).open(&path) {
        *FILE.lock().unwrap() = Some(f);
    }
}

pub fn write(msg: &str) {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    if let Ok(mut guard) = FILE.lock() {
        if let Some(f) = guard.as_mut() {
            let _ = writeln!(f, "[{}.{:03}] {}", (ms / 1000) % 100000, ms % 1000, msg);
            let _ = f.flush();
        }
    }
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::log::write(&format!($($arg)*)) };
}
