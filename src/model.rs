//! Shared data types: a shortcut, what triggers it, and where it was found.

use serde::{Deserialize, Serialize};

pub const CMD: u8 = 1;
pub const SHIFT: u8 = 2;
pub const OPT: u8 = 4;
pub const CTRL: u8 = 8;
pub const FN: u8 = 16;

/// What the person presses.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Trigger {
    /// Modifiers plus one key, such as ⌘⇧T.
    Combo { mods: u8, key: String },
    /// A modifier key pressed by itself `taps` times, such as double-tap ⌥.
    ModTap { mods: u8, taps: u8 },
    /// A modifier key held down by itself, such as hold fn.
    ModHold { mods: u8 },
}

impl Trigger {
    pub fn combo(mods: u8, key: impl Into<String>) -> Self {
        Trigger::Combo { mods, key: key.into() }
    }

    /// The modifier bits for the modifier-only triggers.
    pub fn bare_modifier(&self) -> Option<u8> {
        match self {
            Trigger::ModTap { mods, .. } | Trigger::ModHold { mods } => Some(*mods),
            Trigger::Combo { .. } => None,
        }
    }

    pub fn mods(&self) -> u8 {
        match self {
            Trigger::Combo { mods, .. } | Trigger::ModTap { mods, .. } | Trigger::ModHold { mods } => *mods,
        }
    }
}

/// Global shortcuts work everywhere; app shortcuts only when that app is in front.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Scope {
    Global,
    App,
    /// Only active inside a special mode of a tool (for example an AeroSpace mode).
    Mode,
}

/// How the shortcut was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Origin {
    LiveMenu,
    RememberedMenu,
    MenuFile,
    ElectronBundle,
    SystemSettings,
    AppSettings,
    ConfigFile,
    KeyRemap,
    BuiltInDefault,
}

impl Origin {
    pub fn label(self) -> &'static str {
        match self {
            Origin::LiveMenu => "Live menu",
            Origin::RememberedMenu => "Menu (remembered)",
            Origin::MenuFile => "Menu file in app",
            Origin::ElectronBundle => "App code (Electron)",
            Origin::SystemSettings => "macOS settings",
            Origin::AppSettings => "App settings",
            Origin::ConfigFile => "Config file",
            Origin::KeyRemap => "Key remapper",
            Origin::BuiltInDefault => "App default (guess)",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shortcut {
    pub app: String,
    pub bundle: Option<String>,
    pub action: String,
    pub trigger: Trigger,
    pub scope: Scope,
    pub origin: Origin,
    /// Where exactly it came from (file, menu path, settings key).
    pub detail: String,
    pub enabled: bool,
    pub estimated: bool,
}

impl Shortcut {
    pub fn new(app: &str, action: &str, trigger: Trigger, scope: Scope, origin: Origin) -> Self {
        Shortcut {
            app: app.to_string(),
            bundle: None,
            action: action.to_string(),
            trigger,
            scope,
            origin,
            detail: String::new(),
            enabled: true,
            estimated: false,
        }
    }
    pub fn bundle(mut self, b: Option<&str>) -> Self {
        self.bundle = b.map(|s| s.to_string());
        self
    }
    pub fn detail(mut self, d: impl Into<String>) -> Self {
        self.detail = d.into();
        self
    }
    pub fn estimated(mut self, e: bool) -> Self {
        self.estimated = e;
        self
    }
    #[allow(dead_code)]
    pub fn enabled(mut self, e: bool) -> Self {
        self.enabled = e;
        self
    }
    /// One owner per app (bundle id when known, else name).
    pub fn owner(&self) -> String {
        self.bundle.clone().unwrap_or_else(|| self.app.clone())
    }
}

#[derive(Clone, Debug, Default)]
pub struct AppInfo {
    pub name: String,
    pub bundle: Option<String>,
    pub path: String,
    pub running: bool,
    pub background_only: bool,
    pub has_menu_file: bool,
    pub has_electron: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SourceNote {
    pub name: String,
    pub count: usize,
    pub status: String,
    pub ok: bool,
}
