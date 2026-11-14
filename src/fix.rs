//! How to change a shortcut, so a clash can be removed: which file, settings screen or app to open,
//! and which key combinations near it are still free.

use crate::model::*;
use std::collections::HashSet;
use std::process::Command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FixAction {
    /// Open this config file in the text editor.
    OpenFile(String),
    /// Open System Settings › Keyboard.
    KeyboardSettings,
    /// Open the app that owns the hotkey (bundle id).
    OpenApp(String),
    Nothing,
}

#[derive(Clone, Debug)]
pub struct Fix {
    /// Short text for the link.
    pub label: &'static str,
    /// The full instruction.
    pub how: String,
    pub action: FixAction,
}

/// Where the person changes this shortcut.
pub fn fix_for(s: &Shortcut) -> Fix {
    let app_action = || match &s.bundle {
        Some(b) if b != "*" && b != crate::sources::MACOS && b.contains('.') => FixAction::OpenApp(b.clone()),
        _ => FixAction::Nothing,
    };
    match s.origin {
        Origin::ConfigFile | Origin::KeyRemap => {
            // The detail starts with the file path, then " · " and the line.
            let (file, line) = s.detail.split_once(" · ").unwrap_or((s.detail.as_str(), ""));
            if file.starts_with('/') {
                let at = if line.is_empty() { String::new() } else { format!(" Look for: {line}") };
                Fix { label: "Edit file", how: format!("Change the keys in {file}.{at}"), action: FixAction::OpenFile(file.to_string()) }
            } else {
                Fix { label: "Open app", how: format!("Change this in the settings of {}.", s.app), action: app_action() }
            }
        }
        Origin::SystemSettings => Fix {
            label: "Keyboard settings",
            how: format!("System Settings › Keyboard › Keyboard Shortcuts… Find \"{}\". Change its keys or turn it off.", s.action),
            action: FixAction::KeyboardSettings,
        },
        Origin::AppSettings if s.detail.starts_with("Custom menu shortcut") => Fix {
            label: "Keyboard settings",
            how: format!("You set this one: System Settings › Keyboard › Keyboard Shortcuts… › App Shortcuts › \"{}\".", s.action),
            action: FixAction::KeyboardSettings,
        },
        Origin::LiveMenu | Origin::RememberedMenu | Origin::MenuFile => menu_fix(s),
        Origin::ElectronBundle if s.scope == Scope::App => menu_fix(s),
        Origin::AppSettings | Origin::BuiltInDefault | Origin::ElectronBundle => Fix {
            label: "Open app",
            how: format!("Open {} and change \"{}\" in its settings.", s.app, s.action),
            action: app_action(),
        },
    }
}

/// A menu item has fixed keys; macOS can give it other keys.
fn menu_fix(s: &Shortcut) -> Fix {
    Fix {
        label: "Give other keys",
        how: format!(
            "System Settings › Keyboard › Keyboard Shortcuts… › App Shortcuts › +. Choose {}, type the menu title \"{}\" exactly, then press the new keys.",
            s.app, s.action
        ),
        action: FixAction::KeyboardSettings,
    }
}

pub fn run(action: &FixAction) {
    let _ = match action {
        FixAction::OpenFile(p) => Command::new("/usr/bin/open").args(["-t", p]).spawn(),
        FixAction::KeyboardSettings => Command::new("/usr/bin/open").arg("x-apple.systempreferences:com.apple.Keyboard-Settings.extension").spawn(),
        FixAction::OpenApp(b) => Command::new("/usr/bin/open").args(["-b", b]).spawn(),
        FixAction::Nothing => return,
    };
}

/// Keys of a US keyboard with their row and column, to measure which keys are near.
const ROWS: [(&str, f32); 4] = [("`1234567890-=", 0.0), ("QWERTYUIOP[]\\", 1.5), ("ASDFGHJKL;'", 1.75), ("ZXCVBNM,./", 2.25)];

fn position(key: &str) -> Option<(f32, f32)> {
    let c = key.chars().next().filter(|_| key.chars().count() == 1)?;
    ROWS.iter().enumerate().find_map(|(r, (keys, offset))| keys.chars().position(|k| k == c).map(|i| (i as f32 + offset, r as f32)))
}

/// Every trigger in use (turned on, not only inside a tool's own mode).
pub fn used_triggers(list: &[Shortcut]) -> HashSet<Trigger> {
    list.iter().filter(|s| s.enabled && s.scope != Scope::Mode).map(|s| s.trigger.clone()).collect()
}

/// Up to `n` combinations with the same modifier keys that no app or tool uses, nearest keys first.
pub fn free_keys_near(t: &Trigger, used: &HashSet<Trigger>, n: usize) -> Vec<Trigger> {
    let Trigger::Combo { mods, key } = t else { return Vec::new() };
    // A key by itself, or with Shift only, types text: never suggest it.
    if *mods & !(SHIFT) == 0 {
        return Vec::new();
    }
    let from = position(key).unwrap_or((6.0, 1.5));
    let mut free: Vec<(f32, Trigger)> = ROWS
        .iter()
        .flat_map(|(keys, _)| keys.chars())
        .map(|c| c.to_string())
        .filter(|k| k != key)
        .filter_map(|k| {
            let (x, y) = position(&k)?;
            let cand = Trigger::Combo { mods: *mods, key: k };
            (!used.contains(&cand)).then(|| ((x - from.0).powi(2) + (y - from.1).powi(2), cand))
        })
        .collect();
    free.sort_by(|a, b| a.0.total_cmp(&b.0));
    free.into_iter().take(n).map(|(_, t)| t).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_to_change() {
        let s = Shortcut::new("AeroSpace", "focus left", Trigger::combo(CTRL | OPT | CMD, "Left"), Scope::Global, Origin::ConfigFile)
            .bundle(Some("bobko.aerospace"))
            .detail("/Users/me/.aerospace.toml · [mode.main.binding] ctrl-alt-cmd-left");
        let f = fix_for(&s);
        assert_eq!(f.action, FixAction::OpenFile("/Users/me/.aerospace.toml".into()));
        assert!(f.how.contains("ctrl-alt-cmd-left"));

        let s = Shortcut::new("Rectangle", "Next Display", Trigger::combo(CTRL | OPT | CMD, "Right"), Scope::Global, Origin::BuiltInDefault)
            .bundle(Some("com.knollsoft.Rectangle"));
        assert_eq!(fix_for(&s).action, FixAction::OpenApp("com.knollsoft.Rectangle".into()));

        let s = Shortcut::new("Mail", "Attach Files…", Trigger::combo(CMD | SHIFT, "A"), Scope::App, Origin::MenuFile).bundle(Some("com.apple.mail"));
        let f = fix_for(&s);
        assert_eq!(f.action, FixAction::KeyboardSettings);
        assert!(f.how.contains("Attach Files…"));

        let s = Shortcut::new("macOS", "Mission Control", Trigger::combo(CTRL, "Up"), Scope::Global, Origin::SystemSettings).bundle(Some(crate::sources::MACOS));
        assert_eq!(fix_for(&s).action, FixAction::KeyboardSettings);
    }

    #[test]
    fn free_keys() {
        let t = Trigger::combo(CTRL | OPT, "F");
        let used: HashSet<Trigger> = [t.clone(), Trigger::combo(CTRL | OPT, "G"), Trigger::combo(CTRL | OPT, "D")].into_iter().collect();
        let free = free_keys_near(&t, &used, 4);
        assert_eq!(free.len(), 4);
        assert!(free.iter().all(|f| !used.contains(f) && f.mods() == CTRL | OPT));
        // D and G next to F are taken, so the nearest free key is R, above F.
        assert_eq!(free[0], Trigger::combo(CTRL | OPT, "R"));
        // A plain key is never suggested.
        assert!(free_keys_near(&Trigger::combo(0, "F"), &used, 4).is_empty());
    }
}
