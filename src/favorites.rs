//! Starred shortcuts: the few keys you actually want to use.
//! A star is kept per item, keyed by (app_id, title, value), in a JSON file.
//! Peek shows the starred group at the start of the list.

use crate::keys::trigger_text;
use crate::model::Shortcut;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

fn path() -> std::path::PathBuf {
    crate::sources::data_dir().join("favorites.json")
}

fn store() -> &'static Mutex<HashSet<String>> {
    static S: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    S.get_or_init(|| {
        let set = crate::sources::read_json(&path()).unwrap_or_default();
        Mutex::new(set)
    })
}

/// The key of a star: app id, title, keys.
pub fn key(s: &Shortcut) -> String {
    format!("{}|{}|{}", s.owner(), s.action, trigger_text(&s.trigger))
}

pub fn is_starred(s: &Shortcut) -> bool {
    store().lock().map(|g| g.contains(&key(s))).unwrap_or(false)
}

pub fn toggle(s: &Shortcut) {
    if let Ok(mut g) = store().lock() {
        let k = key(s);
        if !g.remove(&k) {
            g.insert(k);
        }
        if let Ok(b) = serde_json::to_vec_pretty(&*g) {
            let _ = crate::sources::write_atomic(&path(), &b);
        }
    }
}

pub fn count() -> usize {
    store().lock().map(|g| g.len()).unwrap_or(0)
}

// ------------------------------------------------------------ hidden rows

/// Rows the person hid from the Peek panel. Same key as a star.
fn hidden_path() -> std::path::PathBuf {
    crate::sources::data_dir().join("hidden.json")
}

fn hidden_store() -> &'static Mutex<HashSet<String>> {
    static S: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    S.get_or_init(|| {
        let set = crate::sources::read_json(&hidden_path()).unwrap_or_default();
        Mutex::new(set)
    })
}

pub fn is_hidden(s: &Shortcut) -> bool {
    hidden_store().lock().map(|g| g.contains(&key(s))).unwrap_or(false)
}

pub fn toggle_hidden(s: &Shortcut) {
    if let Ok(mut g) = hidden_store().lock() {
        let k = key(s);
        if !g.remove(&k) {
            g.insert(k);
        }
        if let Ok(b) = serde_json::to_vec_pretty(&*g) {
            let _ = crate::sources::write_atomic(&hidden_path(), &b);
        }
    }
}
