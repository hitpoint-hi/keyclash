//! Choices of the person who uses the app: the welcome page was read, the use counter is on,
//! and which key opens the shortcuts panel. Kept in `settings.json` in the data folder.

use crate::keys::{NS_COMMAND, NS_CONTROL, NS_OPTION};
use crate::model::{CMD, CTRL, OPT};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

/// The modifier key that opens the shortcuts panel when it is pressed two times.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelKey {
    #[default]
    Command,
    Option,
    Control,
    /// No key opens the panel.
    Off,
}

impl PanelKey {
    /// The modifier bit (model.rs), or None when the panel key is off.
    pub fn mods(self) -> Option<u8> {
        match self {
            PanelKey::Command => Some(CMD),
            PanelKey::Option => Some(OPT),
            PanelKey::Control => Some(CTRL),
            PanelKey::Off => None,
        }
    }

    /// The NSEvent flag of the key, or 0 when the panel key is off.
    pub fn flag(self) -> i64 {
        match self {
            PanelKey::Command => NS_COMMAND,
            PanelKey::Option => NS_OPTION,
            PanelKey::Control => NS_CONTROL,
            PanelKey::Off => 0,
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            PanelKey::Command => "⌘",
            PanelKey::Option => "⌥",
            PanelKey::Control => "⌃",
            PanelKey::Off => "",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// The welcome page was read.
    #[serde(default)]
    pub welcomed: bool,
    /// Count shortcut presses. Off until the person turns it on.
    #[serde(default)]
    pub count_keys: bool,
    #[serde(default)]
    pub panel_key: PanelKey,
    /// Text size of the Peek panel rows, in points. ⌘ + and ⌘ − change it in the panel.
    #[serde(default = "default_panel_font")]
    pub panel_font: f32,
    /// How held modifier keys pick the rows to highlight: 0 exact, 1 includes, 2 any.
    #[serde(default)]
    pub highlight_mode: u8,
    /// Where the person moved Peek: the window's top-left corner, in points. None: centered.
    #[serde(default)]
    pub panel_pos: Option<[f32; 2]>,
    /// The size the person gave Peek, in points. None: fit the rows.
    #[serde(default)]
    pub panel_size: Option<[f32; 2]>,
    #[serde(default = "default_automatic_update_checks")]
    pub automatic_update_checks: bool,
    #[serde(default)]
    pub dismissed_update: Option<String>,
}

/// Default text size: 13 points.
pub const DEFAULT_PANEL_FONT: f32 = 13.0;
pub const PANEL_FONT_RANGE: (f32, f32) = (11.0, 20.0);

fn default_panel_font() -> f32 {
    DEFAULT_PANEL_FONT
}

fn default_automatic_update_checks() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            welcomed: false,
            count_keys: false,
            panel_key: PanelKey::default(),
            panel_font: DEFAULT_PANEL_FONT,
            highlight_mode: 0,
            panel_pos: None,
            panel_size: None,
            automatic_update_checks: default_automatic_update_checks(),
            dismissed_update: None,
        }
    }
}

fn path() -> std::path::PathBuf {
    crate::sources::data_dir().join("settings.json")
}

/// The NSEvent flag of the panel key, for the key listener thread (0: off).
static PANEL_FLAG: AtomicI64 = AtomicI64::new(NS_COMMAND);

fn store() -> &'static Mutex<Settings> {
    static S: OnceLock<Mutex<Settings>> = OnceLock::new();
    S.get_or_init(|| {
        // A missing or damaged file gives the defaults, with the counter off: counting needs a
        // choice that is on record. A damaged file is kept aside, not written over.
        let s: Settings = crate::sources::read_json(&path()).unwrap_or_default();
        PANEL_FLAG.store(s.panel_key.flag(), Ordering::Relaxed);
        Mutex::new(s)
    })
}

pub fn get() -> Settings {
    store().lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn panel_flag() -> i64 {
    // Read the file before the first use of the flag.
    let _ = store();
    PANEL_FLAG.load(Ordering::Relaxed)
}

/// Change the settings and write them to the file.
pub fn update(change: impl FnOnce(&mut Settings)) {
    if let Ok(mut g) = store().lock() {
        change(&mut g);
        PANEL_FLAG.store(g.panel_key.flag(), Ordering::Relaxed);
        if let Ok(b) = serde_json::to_vec_pretty(&*g) {
            let _ = crate::sources::write_atomic(&path(), &b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_person_starts_with_the_counter_off() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(!s.count_keys && !s.welcomed);
        assert_eq!(s.panel_key, PanelKey::Command);
        // A file from an earlier version has no text size: the default applies.
        assert_eq!(s.panel_font, DEFAULT_PANEL_FONT);
        assert_eq!(s.highlight_mode, 0);
        assert!(s.automatic_update_checks);
        assert!(s.dismissed_update.is_none());
    }

    #[test]
    fn the_panel_key_reads_back() {
        let s = Settings { welcomed: true, count_keys: true, panel_key: PanelKey::Option, panel_font: 15.0, highlight_mode: 2, panel_pos: Some([1600.0, 40.0]), panel_size: Some([520.0, 700.0]), automatic_update_checks: false, dismissed_update: Some("0.1.3".into()) };
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        assert_eq!(PanelKey::Off.flag(), 0);
        assert_eq!(PanelKey::Off.mods(), None);
        assert_eq!(PanelKey::Control.mods(), Some(CTRL));
    }
}
