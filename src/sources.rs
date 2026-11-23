//! Every place a shortcut can be found on this Mac.

use crate::keys::*;
use crate::model::*;
use crate::{ax, electron, nib};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".into()))
}

/// The folder for this app's own files: settings, stars, counts, remembered menus, log.
pub fn data_dir() -> PathBuf {
    data_dir_override().unwrap_or_else(|| home().join("Library/Application Support/Keyclash"))
}

/// Write a data file through a temporary file and a rename, so a stop in the middle keeps the old file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let temp = path.with_file_name(name);
    std::fs::write(&temp, bytes)?;
    std::fs::rename(temp, path)
}

/// Read a JSON data file. None when the file is absent.
/// A file that exists but does not parse is renamed to `<name>.bad-<seconds>` and also gives None,
/// so the next save cannot write over data that a newer version or a repair could still read.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = std::fs::read(path).ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(v) => Some(v),
        Err(e) => {
            let mut name = path.file_name().unwrap_or_default().to_os_string();
            name.push(format!(".bad-{}", now_secs()));
            let aside = path.with_file_name(name);
            eprintln!("keyclash: {} does not parse ({e}); kept as {}", path.display(), aside.display());
            let _ = std::fs::rename(path, aside);
            None
        }
    }
}

fn data_dir_override() -> Option<PathBuf> {
    std::env::var_os("KEYCLASH_DATA_DIR").filter(|p| !p.is_empty()).map(PathBuf::from)
}

/// Earlier versions kept the files in "Shortcut Atlas". Move each file that the new folder
/// does not have, then remove the old folder when it is empty. Runs in the installed app only.
pub fn move_old_data() {
    // An isolated QA profile must never move files from the person's normal profile.
    if data_dir_override().is_some() { return; }
    let old = home().join("Library/Application Support/Shortcut Atlas");
    let Ok(rd) = std::fs::read_dir(&old) else { return };
    let new = data_dir();
    if std::fs::create_dir_all(&new).is_err() {
        return;
    }
    for e in rd.flatten() {
        let to = new.join(e.file_name());
        if !to.exists() {
            let _ = std::fs::rename(e.path(), to);
        }
    }
    // Fails when a file stayed: then the old folder stays too.
    let _ = std::fs::remove_dir(&old);
}

fn read_plist(p: &Path) -> Option<plist::Value> {
    plist::Value::from_file(p).ok()
}

fn int_of(v: &plist::Value) -> Option<i64> {
    match v {
        plist::Value::Integer(i) => i.as_signed(),
        plist::Value::Boolean(b) => Some(*b as i64),
        plist::Value::Real(r) => Some(*r as i64),
        plist::Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

// ---------------------------------------------------------------- installed apps

pub fn installed_apps() -> Vec<AppInfo> {
    let h = home();
    let roots = [
        PathBuf::from("/Applications"),
        PathBuf::from("/Applications/Utilities"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
        PathBuf::from("/System/Library/CoreServices/Finder.app"),
        h.join("Applications"),
    ];
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |p: PathBuf, out: &mut Vec<AppInfo>| {
        let canon = std::fs::canonicalize(&p).unwrap_or(p.clone());
        if !seen.insert(canon.clone()) {
            return;
        }
        let info = read_plist(&canon.join("Contents/Info.plist"));
        let d = info.as_ref().and_then(|v| v.as_dictionary());
        let s = |k: &str| d.and_then(|d| d.get(k)).and_then(|v| v.as_string()).map(String::from);
        let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let name = s("CFBundleDisplayName").or_else(|| s("CFBundleName")).filter(|n| !n.is_empty()).unwrap_or(stem);
        let bg = d.and_then(|d| d.get("LSUIElement").or_else(|| d.get("LSBackgroundOnly"))).and_then(int_of).unwrap_or(0) != 0;
        out.push(AppInfo {
            name,
            bundle: s("CFBundleIdentifier"),
            path: canon.to_string_lossy().to_string(),
            running: false,
            background_only: bg,
            has_menu_file: false,
            has_electron: canon.join("Contents/Resources/app.asar").exists(),
        });
    };
    for r in roots {
        if r.extension().is_some_and(|e| e == "app") {
            add(r, &mut out);
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&r) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|e| e == "app") {
                add(p, &mut out);
            } else if p.is_dir() && !p.to_string_lossy().ends_with("Utilities") {
                // One folder level down ("Adobe Acrobat DC/…app").
                if let Ok(inner) = std::fs::read_dir(&p) {
                    for ie in inner.flatten() {
                        if ie.path().extension().is_some_and(|e| e == "app") {
                            add(ie.path(), &mut out);
                        }
                    }
                }
            }
        }
    }
    use_file_names_for_twins(&mut out);
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

/// Two different apps can declare the same display name ("ChatGPT.app" and "ChatGPT Classic.app"
/// both say "ChatGPT"). Finder shows the file name, so use the file name for such apps.
pub fn use_file_names_for_twins(apps: &mut [AppInfo]) {
    let mut ids: HashMap<String, HashSet<String>> = HashMap::new();
    for a in apps.iter() {
        ids.entry(a.name.to_lowercase()).or_default().insert(a.bundle.clone().unwrap_or_else(|| a.path.clone()));
    }
    for a in apps.iter_mut() {
        if ids[&a.name.to_lowercase()].len() < 2 {
            continue;
        }
        let stem = Path::new(&a.path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if !stem.is_empty() {
            a.name = stem;
        }
    }
}

pub fn app_name_for(apps: &[AppInfo], bundle: &str) -> String {
    apps.iter()
        .find(|a| a.bundle.as_deref().is_some_and(|b| b.eq_ignore_ascii_case(bundle)))
        .map(|a| a.name.clone())
        .unwrap_or_else(|| {
            // com.knollsoft.Rectangle -> Rectangle
            bundle.rsplit('.').next().unwrap_or(bundle).to_string()
        })
}

fn has_app(apps: &[AppInfo], bundle: &str) -> bool {
    apps.iter().any(|a| a.bundle.as_deref() == Some(bundle))
}

// ---------------------------------------------------------------- macOS shortcuts

/// Owner id used for macOS's own shortcuts.
pub const MACOS: &str = "com.apple.macos";

/// "No key" marker in symbolichotkeys parameters (0xffff).
pub const NO_KEY: i64 = 65535;

const SPACES_PATHS: [&str; 2] = [
    "/System/Library/ExtensionKit/Extensions/KeyboardSettings.appex/Contents/Resources/DefaultSpacesShortcuts.xml",
    "/System/Library/PreferencePanes/Keyboard.prefPane/Contents/Resources/DefaultSpacesShortcuts.xml",
];

const TABLE_PATHS: [&str; 2] = [
    "/System/Library/ExtensionKit/Extensions/KeyboardSettings.appex/Contents/Resources/en.lproj/DefaultShortcutsTable.xml",
    "/System/Library/PreferencePanes/Keyboard.prefPane/Contents/Resources/en.lproj/DefaultShortcutsTable.xml",
];

fn fallback_system_name(id: i64) -> String {
    match id {
        28 => "Save picture of screen as a file".into(),
        29 => "Copy picture of screen to the clipboard".into(),
        30 => "Save picture of selected area as a file".into(),
        31 => "Copy picture of selected area to the clipboard".into(),
        32 => "Mission Control".into(),
        33 => "Application windows".into(),
        36 => "Show Desktop".into(),
        60 => "Select the previous input source".into(),
        61 => "Select next source in Input menu".into(),
        64 => "Show Spotlight search".into(),
        65 => "Show Finder search window".into(),
        79 => "Move left a space".into(),
        81 => "Move right a space".into(),
        118..=133 => format!("Switch to Desktop {}", id - 117),
        98 => "Show Help menu".into(),
        184 => "Screenshot and recording options".into(),
        _ => format!("macOS shortcut #{id}"),
    }
}

/// One row of macOS's own shortcut table.
struct SystemDef {
    name: String,
    group: String,
    key: i64,
    mods: i64,
    /// The character macOS types for the key (`charKey`), when the table gives one.
    char_key: Option<i64>,
    /// macOS turns it on until the person changes it.
    default_on: bool,
}

/// Read one entry of DefaultShortcutsTable.xml and the entries nested inside it
/// ("Windows › Halves", "Expose and Spaces › Spaces"). "enabled" passes down to nested entries.
fn collect_system_defs(e: &plist::Dictionary, group: &str, inherited_on: bool, out: &mut HashMap<i64, SystemDef>) {
    let text = |k: &str| e.get(k).and_then(|v| v.as_string()).unwrap_or("").replace("DO_NOT_LOCALIZE: ", "");
    let on = e.get("enabled").and_then(int_of).map(|v| v != 0).unwrap_or(inherited_on);
    let name = text("name");
    if let Some(id) = e.get("sybmolichotkey").and_then(int_of) {
        // Shortcuts for hardware or input methods this Mac may not have.
        let ident = text("identifier");
        let special = ident == "TrackpadHandwriting" || ident.starts_with("touchbar.");
        out.insert(id, SystemDef {
            name: name.clone(),
            group: group.to_string(),
            key: e.get("key").and_then(int_of).unwrap_or(NO_KEY),
            mods: e.get("modifier").and_then(int_of).unwrap_or(0),
            char_key: e.get("charKey").and_then(int_of),
            default_on: on && !special,
        });
    }
    let Some(children) = e.get("elements").and_then(|v| v.as_array()) else { return };
    let sub = if group.is_empty() { name } else if name.is_empty() { group.to_string() } else { format!("{group} › {name}") };
    for c in children {
        if let Some(cd) = c.as_dictionary() {
            collect_system_defs(cd, &sub, on, out);
        }
    }
}

/// Clearer names for a few entries of macOS's table.
fn system_name(id: i64, table_name: &str) -> String {
    match id {
        32 => "Mission Control".into(),
        36 => "Show Desktop".into(),
        190 => "Quick Note".into(),
        260 => "Game Overlay".into(),
        _ if table_name.is_empty() => fallback_system_name(id),
        _ => table_name.to_string(),
    }
}

pub fn system_shortcuts() -> (Vec<Shortcut>, SourceNote) {
    let mut defs: HashMap<i64, SystemDef> = HashMap::new();
    let mut table_found = false;
    for p in TABLE_PATHS {
        let Some(v) = read_plist(Path::new(p)) else { continue };
        table_found = true;
        for g in v.as_array().into_iter().flatten() {
            if let Some(gd) = g.as_dictionary() {
                collect_system_defs(gd, "", true, &mut defs);
            }
        }
        break;
    }
    // Spaces shortcuts: a flat list of the same elements.
    // "Switch to Desktop N" is off until the person turns it on (macOS default).
    for p in SPACES_PATHS {
        let Some(v) = read_plist(Path::new(p)) else { continue };
        for e in v.as_array().into_iter().flatten() {
            let Some(ed) = e.as_dictionary() else { continue };
            let mut one = HashMap::new();
            collect_system_defs(ed, "Mission Control", false, &mut one);
            for (id, d) in one {
                defs.entry(id).or_insert(d);
            }
        }
        break;
    }
    // Ids missing from both tables. Keys are macOS defaults (estimated).
    for (id, name, key, mods) in [(79, "Move left a space", 123, 262144), (81, "Move right a space", 124, 262144)] {
        defs.entry(id).or_insert(SystemDef { name: name.into(), group: "Mission Control".into(), key, mods, char_key: None, default_on: true });
    }
    let user = read_plist(&home().join("Library/Preferences/com.apple.symbolichotkeys.plist"));
    let user_map = user
        .as_ref()
        .and_then(|v| v.as_dictionary())
        .and_then(|d| d.get("AppleSymbolicHotKeys"))
        .and_then(|v| v.as_dictionary())
        .cloned()
        .unwrap_or_default();
    let out = system_shortcuts_from(&defs, &user_map);
    let on = out.len();
    let defaults = out.iter().filter(|s| s.estimated).count();
    let note = SourceNote {
        name: "macOS keyboard shortcuts".into(),
        count: on,
        status: if table_found {
            format!("{} set in your Keyboard Shortcuts settings, {defaults} macOS defaults you never changed (guess)", on - defaults)
        } else {
            "name table not found; ids only".into()
        },
        ok: true,
    };
    (out, note)
}

/// Which macOS shortcuts are on, from macOS's table and the person's settings.
///
/// A stored entry counts only when: "enabled" is true, "value" is a
/// dictionary with "type" == "standard" and exactly 3 "parameters", and neither the key code
/// (parameters[1]) nor the modifiers (parameters[2]) is 65535.
/// Also: macOS stores an entry only after the person changes it. An entry that
/// is on by default and absent from the settings still works with its default keys, so it is listed (marked "guess").
fn system_shortcuts_from(defs: &HashMap<i64, SystemDef>, user_map: &plist::Dictionary) -> Vec<Shortcut> {
    let mut ids: Vec<i64> = user_map.keys().filter_map(|k| k.parse::<i64>().ok()).chain(defs.keys().copied()).collect();
    ids.sort();
    ids.dedup();
    let mut out = Vec::new();
    for id in ids {
        let def = defs.get(&id);
        let table_keys = |why: &'static str| def.filter(|d| d.key != NO_KEY).map(|d| (d.key, d.mods, d.char_key, Some(why)));
        let found = match user_map.get(&id.to_string()).and_then(|v| v.as_dictionary()) {
            Some(u) => {
                let enabled = u.get("enabled").and_then(int_of).map(|e| e != 0).unwrap_or(false);
                if !enabled {
                    continue;
                }
                match u.get("value").and_then(|v| v.as_dictionary()) {
                    Some(value) => {
                        if value.get("type").and_then(|v| v.as_string()) != Some("standard") {
                            continue;
                        }
                        let params: Vec<i64> = value
                            .get("parameters")
                            .and_then(|v| v.as_array())
                            .map(|a| a.iter().filter_map(int_of).collect())
                            .unwrap_or_default();
                        if params.len() != 3 || params[1] == NO_KEY || params[2] == NO_KEY {
                            continue;
                        }
                        Some((params[1], params[2], Some(params[0]), None))
                    }
                    // "On" with no stored keys: macOS uses its default keys.
                    None => table_keys("turned on, macOS default keys"),
                }
            }
            // Never changed: on only when macOS turns it on by default.
            None if def.is_some_and(|d| d.default_on) => table_keys("macOS default, never changed"),
            None => None,
        };
        let Some((key, mods, char_key, from_table)) = found else { continue };
        let group = def.map(|d| d.group.clone()).unwrap_or_default();
        // macOS matches these keys by the typed character, not the key position, when the
        // entry has one. On Colemak-DH, key code 46 types H (not M), and Dock hiding works
        // with the key that types D (tested on Bobby's Mac, 2026-11-13). Arrows, F-keys and
        // Space carry 65535 or a control character, so the key code decides for them.
        let Some(kname) = char_key.and_then(menu_char).or_else(|| keycode_key(key)) else { continue };
        let kname = kname.as_str();
        // Stored values carry the fn flag on every arrow and F-key, so it is removed there.
        // In macOS's table the fn flag is the Globe key itself (fn ⌃ ← tiles a window).
        let trigger = match from_table {
            Some(_) => Trigger::combo(mods_from_nsevent(mods), kname),
            None => clean_trigger(mods_from_nsevent(mods), kname.to_string()),
        };
        let name = system_name(id, def.map(|d| d.name.as_str()).unwrap_or(""));
        out.push(
            Shortcut::new("macOS", &name, trigger, Scope::Global, Origin::SystemSettings)
                .bundle(Some(MACOS))
                .detail(format!(
                    "Keyboard Shortcuts{} · id {id}{}",
                    if group.is_empty() { String::new() } else { format!(" › {group}") },
                    from_table.map(|w| format!(" · {w}")).unwrap_or_default()
                ))
                .estimated(from_table.is_some()),
        );
    }
    out
}

/// The key name of a character from macOS's shortcut settings ("m" gives "M"). None for 65535 or a control character.
fn menu_char(c: i64) -> Option<String> {
    let ch = char::from_u32(u32::try_from(c).ok()?)?;
    ch.is_ascii_graphic().then(|| ch.to_ascii_uppercase().to_string())
}

/// The fn / Globe key setting ("Press 🌐 key to …").
pub fn fn_key_usage() -> Vec<Shortcut> {
    let Some(v) = read_plist(&home().join("Library/Preferences/com.apple.HIToolbox.plist")) else { return vec![] };
    let usage = v.as_dictionary().and_then(|d| d.get("AppleFnUsageType")).and_then(int_of);
    let action = match usage {
        Some(1) => "Change input source",
        Some(2) => "Show Emoji & Symbols",
        Some(3) => "Start Dictation",
        _ => return vec![],
    };
    vec![Shortcut::new("macOS", action, Trigger::ModTap { mods: FN, taps: 1 }, Scope::Global, Origin::SystemSettings)
        .bundle(Some(MACOS))
        .detail("Keyboard › Press fn/Globe key to")]
}

// ---------------------------------------------------------------- app settings

/// "@$~^k" style key equivalents from NSUserKeyEquivalents.
pub fn parse_user_key_equivalent(s: &str) -> Option<Trigger> {
    let mut mods = 0;
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        let m = match c {
            '@' => CMD,
            '$' => SHIFT,
            '~' => OPT,
            '^' => CTRL,
            _ => break,
        };
        if rest.len() == 1 {
            break;
        }
        mods |= m;
        rest = &rest[c.len_utf8()..];
    }
    let (key, shift) = key_from_char(rest)?;
    Some(clean_trigger(mods | if shift { SHIFT } else { 0 }, key))
}

fn pretty_key_name(k: &str) -> String {
    let k = k.trim_start_matches("KeyboardShortcuts_").replace(['_', '.'], " ");
    let mut out = String::new();
    let mut prev_lower = false;
    for c in k.chars() {
        if c.is_uppercase() && prev_lower {
            out.push(' ');
        }
        prev_lower = c.is_lowercase() || c.is_ascii_digit();
        out.push(c);
    }
    let mut c = out.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => out,
    }
}

/// The action name for a settings key: "hotkeyQuickCapture" gives "Quick Capture", "hotkey" gives "Main hotkey".
fn hotkey_name(path: &str) -> String {
    let name = pretty_key_name(path);
    match name.strip_prefix("Hotkey ").or_else(|| name.strip_prefix("Hot Key ")) {
        Some(rest) if !rest.is_empty() => rest.to_string(),
        _ if name.eq_ignore_ascii_case("hotkey") => "Main hotkey".to_string(),
        _ => name,
    }
}

/// Recognise a stored hotkey in any settings value.
fn hotkey_in_value(v: &plist::Value) -> Option<Trigger> {
    match v {
        plist::Value::Dictionary(d) => {
            let code = d.get("keyCode").or_else(|| d.get("keycode")).or_else(|| d.get("carbonKeyCode")).and_then(int_of)?;
            let flags = d
                .get("modifierFlags")
                .or_else(|| d.get("modifiers"))
                .or_else(|| d.get("carbonModifiers"))
                .and_then(int_of)
                .unwrap_or(0);
            let key = keycode_key(code)?;
            let key = key.as_str();
            let mods = if d.contains_key("carbonModifiers") || (flags > 0 && flags < 0x10000) {
                mods_from_carbon(flags)
            } else {
                mods_from_nsevent(flags)
            };
            if mods == 0 && !is_function_key(key) {
                return None;
            }
            Some(clean_trigger(mods, key.to_string()))
        }
        plist::Value::String(s) if s.contains("carbonKeyCode") || s.contains("keyCode") => {
            let j: serde_json::Value = serde_json::from_str(s).ok()?;
            let code = j.get("carbonKeyCode").or_else(|| j.get("keyCode"))?.as_i64()?;
            let flags = j.get("carbonModifiers").or_else(|| j.get("modifiers")).and_then(|v| v.as_i64()).unwrap_or(0);
            let key = keycode_key(code)?;
            Some(clean_trigger(mods_from_carbon(flags), key))
        }
        plist::Value::Data(d) if d.len() < 4096 && d.starts_with(b"bplist") => {
            // MASShortcut / ShortcutRecorder archives keep keyCode + modifierFlags inside a keyed archive.
            let inner = plist::Value::from_reader(std::io::Cursor::new(d)).ok()?;
            let objs = inner.as_dictionary()?.get("$objects")?.as_array()?;
            for o in objs {
                if let Some(t) = hotkey_in_value(o) {
                    return Some(t);
                }
            }
            None
        }
        _ => None,
    }
}

/// Hotkeys stored as two values with one name: `hotkeyQuickCaptureKeyCode` = 5 and
/// `hotkeyQuickCaptureModifiers` = 768 (macshot). Returns (name, keys).
fn paired_hotkeys(d: &plist::Dictionary) -> Vec<(String, Trigger)> {
    let mut out = Vec::new();
    for (k, v) in d {
        let Some(name) = k.strip_suffix("KeyCode").or_else(|| k.strip_suffix("Keycode")).filter(|n| !n.is_empty()) else { continue };
        let Some(code) = int_of(v) else { continue };
        let Some(flags) = d.get(&format!("{name}Modifiers")).or_else(|| d.get(&format!("{name}ModifierFlags"))).and_then(int_of) else { continue };
        let Some(key) = keycode_key(code) else { continue };
        let mods = if flags > 0 && flags < 0x10000 { mods_from_carbon(flags) } else { mods_from_nsevent(flags) };
        // Key code 0 with no modifier keys is an empty slot, not the A key.
        if mods == 0 && !is_function_key(&key) {
            continue;
        }
        out.push((name.to_string(), clean_trigger(mods, key)));
    }
    out
}

fn walk_settings(v: &plist::Value, path: &str, depth: usize, out: &mut Vec<(String, Trigger)>) {
    if depth > 3 {
        return;
    }
    if let Some(t) = hotkey_in_value(v) {
        out.push((path.to_string(), t));
        return;
    }
    if let plist::Value::Dictionary(d) = v {
        for (name, t) in paired_hotkeys(d) {
            out.push((if path.is_empty() { name } else { format!("{path}.{name}") }, t));
        }
        for (k, val) in d {
            if k.starts_with("NS") && k != "NSUserKeyEquivalents" || k.starts_with("SU") || k.starts_with("com.apple") {
                continue;
            }
            let p = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
            walk_settings(val, &p, depth + 1, out);
        }
    }
}

/// MacShot 4.4's HotkeyManager.HotkeySlot maps stored chords to numbered disabled flags.
fn macshot_hotkey_slot(domain: &str, key: &str) -> Option<(u8, &'static str)> {
    if !matches!(domain, "com.sw33tlie.macshot.macshot" | "com.sw33tlie.macshot.offline") {
        return None;
    }
    Some(match key {
        "hotkey" => (1, "Capture Area"),
        "hotkeyFullScreen" => (2, "Capture Screen"),
        "hotkeyRecord" => (3, "Record Area"),
        "hotkeyRecordFullScreen" => (4, "Record Screen"),
        "hotkeyHistory" => (5, "History"),
        "hotkeyOCR" => (6, "Capture OCR & QR"),
        "hotkeyQuickCapture" => (7, "Quick Capture"),
        "hotkeyScrollCapture" => (8, "Scroll Capture"),
        "hotkeyOpenClipboard" => (9, "Open from Clipboard"),
        "hotkeyCaptureLastArea" => (10, "Capture Last Area"),
        "hotkeyPinClipboard" => (11, "Pin from Clipboard"),
        "hotkeyClearHistory" => (12, "Clear History"),
        "hotkeyRecordGIF" => (13, "Record GIF"),
        _ => return None,
    })
}

/// Shortcuts stored in one settings domain: menu overrides and hotkeys.
fn shortcuts_in_domain(apps: &[AppInfo], domain: &str, v: &plist::Value, file: &str) -> Vec<Shortcut> {
    let mut out = Vec::new();
    let Some(d) = v.as_dictionary() else { return out };
    let is_global = domain == ".GlobalPreferences";
    let app = if is_global { "Every app".to_string() } else { app_name_for(apps, domain) };
    if let Some(ue) = d.get("NSUserKeyEquivalents").and_then(|v| v.as_dictionary()) {
        for (title, val) in ue {
            let Some(s) = val.as_string() else { continue };
            let Some(t) = parse_user_key_equivalent(s) else { continue };
            out.push(
                Shortcut::new(&app, title.trim_start_matches('\u{1b}'), t, Scope::App, Origin::AppSettings)
                    .bundle(Some(if is_global { "*" } else { domain }))
                    .detail(format!("Custom menu shortcut (System Settings › Keyboard › App Shortcuts) · {file}")),
            );
        }
    }
    if domain.starts_with("com.apple.") || is_global {
        return out;
    }
    let mut found = Vec::new();
    walk_settings(v, "", 0, &mut found);
    // Unknown formats can have numbered disabled flags without a known mapping to their actions.
    let some_off = d.keys().any(|k| k.to_lowercase().contains("hotkeydisabled"));
    for (path, t) in found {
        let slot = macshot_hotkey_slot(domain, &path);
        let disabled = slot.is_some_and(|(id, _)| d.get(&format!("hotkeyDisabled_{id}")).and_then(plist::Value::as_boolean) == Some(true));
        let estimated = some_off && slot.is_none();
        let action = slot.map(|(_, label)| label.to_string()).unwrap_or_else(|| hotkey_name(&path));
        // Keys that only work while the tool's own window is open cannot clash with other apps.
        let (scope, note) = if window_only_key(domain, &path) { (Scope::Mode, " · only while its window is open") } else { (Scope::Global, "") };
        let off_note = if disabled { " · turned off in MacShot" } else if estimated { " · the app has some hotkeys turned off, and Keyclash cannot tell which" } else { "" };
        out.push(
            Shortcut::new(&app, &action, t, scope, Origin::AppSettings)
                .bundle(Some(domain))
                .detail(format!("Settings key \"{path}\" · {file}{note}{off_note}"))
                .enabled(!disabled)
                .estimated(estimated),
        );
    }
    out
}

/// Maccy: only "popup" is a system-wide hotkey. Pin, delete and preview act on the open
/// clipboard list (Maccy's settings name them "in the open window"). Estimated from Maccy's settings.
pub fn window_only_key(domain: &str, key: &str) -> bool {
    domain == "org.p0deje.Maccy" && key.starts_with("KeyboardShortcuts_") && key != "KeyboardShortcuts_popup"
}

pub fn settings_shortcuts(apps: &[AppInfo]) -> (Vec<Shortcut>, SourceNote) {
    let dir = home().join("Library/Preferences");
    let files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "plist"))
                .filter(|e| e.file_name() != "com.apple.symbolichotkeys.plist")
                // Huge files are caches, not settings with hotkeys.
                .filter(|e| e.metadata().map(|m| m.len() <= 1_500_000).unwrap_or(false))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    let per_file = par_map(&files, 6, |p| {
        let domain = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
        read_plist(p).map(|v| shortcuts_in_domain(apps, &domain, &v, &format!("~/Library/Preferences/{domain}.plist")))
    });
    let read = per_file.iter().filter(|x| x.is_some()).count();
    let out: Vec<Shortcut> = per_file.into_iter().flatten().flatten().collect();
    let n = out.len();
    (out, SourceNote { name: "App settings files".into(), count: n, status: format!("{read} settings files read"), ok: true })
}

/// Sandboxed apps keep settings in containers. macOS guards the data of other apps: the first
/// read of a container file makes macOS ask the person ("… would like to access data from other
/// apps"). `defaults export` does not ask. For an app without that permission it gives back an
/// empty list with no error, so the file is read first.
pub fn container_settings_shortcuts(apps: &[AppInfo], already: &HashSet<String>) -> (Vec<Shortcut>, SourceNote) {
    let dir = home().join("Library/Containers");
    let mut out = Vec::new();
    let mut read = 0;
    let mut blocked = 0;
    // Installed apps among the blocked ones, to name them: (not open now, name).
    let mut blocked_apps: Vec<(bool, String)> = Vec::new();
    let mut domains: Vec<String> = apps.iter().filter_map(|a| a.bundle.clone()).collect();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            domains.push(e.file_name().to_string_lossy().to_string());
        }
    }
    domains.sort();
    domains.dedup();
    for domain in domains {
        if domain.starts_with("com.apple.") || already.contains(&domain) {
            continue;
        }
        if !dir.join(&domain).exists() {
            continue;
        }
        let file = dir.join(&domain).join(format!("Data/Library/Preferences/{domain}.plist"));
        let direct = match std::fs::read(&file) {
            Ok(bytes) => plist::Value::from_reader(std::io::Cursor::new(bytes)).ok(),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                blocked += 1;
                if let Some(a) = apps.iter().find(|a| a.bundle.as_deref() == Some(domain.as_str())) {
                    blocked_apps.push((!a.running, a.name.clone()));
                }
                None
            }
            // The app has no settings file.
            Err(_) => None,
        };
        let v = match direct {
            Some(v) => v,
            None => {
                let Ok(o) = Command::new("/usr/bin/defaults").args(["export", &domain, "-"]).output() else { continue };
                if !o.status.success() {
                    continue;
                }
                let Ok(v) = plist::Value::from_reader(std::io::Cursor::new(o.stdout)) else { continue };
                v
            }
        };
        read += 1;
        out.extend(shortcuts_in_domain(apps, &domain, &v, "app container settings"));
    }
    let n = out.len();
    let (status, ok) = if blocked > 0 {
        // Apps that are open now come first: their hotkeys are active.
        blocked_apps.sort_by_key(|(closed, name)| (*closed, name.to_lowercase()));
        let blocked_apps: Vec<String> = blocked_apps.into_iter().map(|(_, n)| n).collect();
        let names = if blocked_apps.is_empty() {
            String::new()
        } else {
            let more = if blocked_apps.len() > 4 { ", …" } else { "" };
            format!(" ({}{more})", blocked_apps.iter().take(4).cloned().collect::<Vec<_>>().join(", "))
        };
        (
            format!("macOS blocked the settings of {blocked} sandboxed apps{names}. Their hotkeys are not in the list. To read them, allow Keyclash in System Settings › Privacy & Security › Full Disk Access, then scan again."),
            false,
        )
    } else {
        (format!("{read} app containers read"), true)
    };
    (out, SourceNote { name: "Sandboxed app settings".into(), count: n, status, ok })
}

/// Rectangle keeps only changed shortcuts in its settings; the rest are its built-in defaults.
pub fn rectangle_defaults(apps: &[AppInfo], stored: &[Shortcut]) -> Vec<Shortcut> {
    const B: &str = "com.knollsoft.Rectangle";
    if !has_app(apps, B) {
        return vec![];
    }
    let prefs = read_plist(&home().join(format!("Library/Preferences/{B}.plist")));
    let alt = prefs
        .as_ref()
        .and_then(|v| v.as_dictionary())
        .and_then(|d| d.get("alternateDefaultShortcuts"))
        .and_then(int_of)
        .unwrap_or(1)
        != 0;
    let co = CTRL | OPT;
    let oc = OPT | CMD;
    let table: Vec<(&str, &str, u8, &str)> = if alt {
        vec![
            ("leftHalf", "Left Half", co, "Left"), ("rightHalf", "Right Half", co, "Right"),
            ("topHalf", "Top Half", co, "Up"), ("bottomHalf", "Bottom Half", co, "Down"),
            ("topLeft", "Top Left", co, "U"), ("topRight", "Top Right", co, "I"),
            ("bottomLeft", "Bottom Left", co, "J"), ("bottomRight", "Bottom Right", co, "K"),
            ("firstThird", "First Third", co, "D"), ("centerThird", "Center Third", co, "F"),
            ("lastThird", "Last Third", co, "G"), ("firstTwoThirds", "First Two Thirds", co, "E"),
            ("lastTwoThirds", "Last Two Thirds", co, "T"), ("maximize", "Maximize", co, "Return"),
            ("maximizeHeight", "Maximize Height", co | SHIFT, "Up"), ("larger", "Make Larger", co, "="),
            ("smaller", "Make Smaller", co, "-"), ("center", "Center", co, "C"),
            ("restore", "Restore", co, "Delete"), ("nextDisplay", "Next Display", co | CMD, "Right"),
            ("previousDisplay", "Previous Display", co | CMD, "Left"),
        ]
    } else {
        vec![
            ("leftHalf", "Left Half", oc, "Left"), ("rightHalf", "Right Half", oc, "Right"),
            ("topHalf", "Top Half", oc, "Up"), ("bottomHalf", "Bottom Half", oc, "Down"),
            ("maximize", "Maximize", oc, "F"), ("center", "Center", oc, "C"),
            ("nextDisplay", "Next Display", co | CMD, "Right"), ("previousDisplay", "Previous Display", co | CMD, "Left"),
            ("larger", "Make Larger", co | SHIFT, "Right"), ("smaller", "Make Smaller", co | SHIFT, "Left"),
            ("restore", "Restore", co, "Delete"),
        ]
    };
    let changed: HashSet<String> = stored
        .iter()
        .filter(|s| s.bundle.as_deref() == Some(B))
        .map(|s| s.action.to_lowercase().replace(' ', ""))
        .collect();
    let name = app_name_for(apps, B);
    table
        .into_iter()
        .filter(|(k, ..)| !changed.contains(&k.to_lowercase()))
        .map(|(k, label, m, key)| {
            // Rectangle stores key codes, so its defaults stay on the physical keys.
            Shortcut::new(&name, label, physical(Trigger::combo(m, key)), Scope::Global, Origin::BuiltInDefault)
                .bundle(Some(B))
                .detail(format!("Rectangle default for \"{k}\" (not stored in its settings)"))
                .estimated(true)
        })
        .collect()
}

// ---------------------------------------------------------------- config files

fn first_existing(paths: &[PathBuf]) -> Option<PathBuf> {
    paths.iter().find(|p| p.exists()).cloned()
}

/// A trigger that a tool names by the key's place on a US keyboard (AeroSpace, Karabiner, Rectangle),
/// as the key that sits there in `layout`. With Colemak DH, the place of US "E" types F.
fn at_us_position(t: Trigger, layout: Option<&crate::layout::Layout>) -> Trigger {
    match t {
        Trigger::Combo { mods, key } => Trigger::Combo { mods, key: crate::layout::key_on_us_position(&key, layout) },
        other => other,
    }
}

/// `at_us_position` for the keyboard layout in use.
fn physical(t: Trigger) -> Trigger {
    crate::layout::with_active(|layout| at_us_position(t, layout))
}

/// A key named by its place on a US keyboard ("slash" is the key right of the period), as the
/// key of `layout`. The US Shift table applies first, because the name is a US place, not a character.
fn by_us_position(mods: u8, key: String, layout: Option<&crate::layout::Layout>) -> Trigger {
    at_us_position(clean_trigger_us(mods, key), layout)
}

/// `parse_dash_binding` for keys named by US place (AeroSpace with the qwerty preset), in the layout in use.
pub(crate) fn parse_dash_position(s: &str) -> Option<Trigger> {
    let (mods, key) = dash_parts(s)?;
    Some(crate::layout::with_active(|layout| by_us_position(mods, key, layout)))
}

/// "cmd-ctrl-alt-shift-left" (AeroSpace) or "cmd + alt - h" (skhd) style, with keys named by character.
pub(crate) fn parse_dash_binding(s: &str) -> Option<Trigger> {
    let (mods, key) = dash_parts(s)?;
    Some(clean_trigger(mods, key))
}

/// The modifier keys and the key name of a dash binding, before any Shift rule.
fn dash_parts(s: &str) -> Option<(u8, String)> {
    let parts: Vec<&str> = s.split('-').collect();
    let (key_part, mod_parts) = if s.ends_with("--") {
        ("-", &parts[..parts.len().saturating_sub(2)])
    } else {
        (*parts.last()?, &parts[..parts.len() - 1])
    };
    let mut mods = 0;
    for m in mod_parts {
        for w in m.split('+') {
            if w.trim().is_empty() {
                continue;
            }
            mods |= modifier_word(w)?;
        }
    }
    let key = normalize_key(key_part.trim());
    if key.is_empty() {
        return None;
    }
    Some((mods, key))
}

pub fn aerospace() -> Vec<Shortcut> {
    let h = home();
    let Some(p) = first_existing(&[h.join(".aerospace.toml"), h.join(".config/aerospace/aerospace.toml")]) else { return vec![] };
    let Ok(text) = std::fs::read_to_string(&p) else { return vec![] };
    let Ok(v) = text.parse::<toml::Table>() else { return vec![] };
    let mut out = Vec::new();
    let Some(modes) = v.get("mode").and_then(|m| m.as_table()) else { return out };
    // With the default "qwerty" preset, a key name is a place on a US keyboard, in every macOS layout.
    let by_position = v.get("key-mapping").and_then(|k| k.get("preset")).and_then(|p| p.as_str()).is_none_or(|p| p == "qwerty");
    for (mode, mt) in modes {
        let Some(b) = mt.get("binding").and_then(|b| b.as_table()) else { continue };
        for (k, act) in b {
            let t = if by_position { parse_dash_position(k) } else { parse_dash_binding(k) };
            let Some(t) = t else { continue };
            let action = match act {
                toml::Value::String(s) => s.clone(),
                toml::Value::Array(a) => a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("; "),
                _ => continue,
            };
            let scope = if mode == "main" { Scope::Global } else { Scope::Mode };
            out.push(
                Shortcut::new("AeroSpace", &action, t, scope, Origin::ConfigFile)
                    .bundle(Some("bobko.aerospace"))
                    .detail(format!("{} · [mode.{mode}.binding] {k}", p.display())),
            );
        }
    }
    out
}

/// "Control+Option+Command+Left Arrow" (OmniWM).
pub(crate) fn parse_plus_binding(s: &str) -> Option<Trigger> {
    let parts: Vec<&str> = s.split('+').collect();
    let mut mods = 0;
    for m in &parts[..parts.len() - 1] {
        mods |= modifier_word(m)?;
    }
    let key = normalize_key(parts.last()?);
    if key.is_empty() {
        return None;
    }
    Some(clean_trigger(mods, key))
}

pub fn omniwm() -> Vec<Shortcut> {
    let p = home().join(".config/omniwm/settings.toml");
    let Ok(text) = std::fs::read_to_string(&p) else { return vec![] };
    let Ok(v) = text.parse::<toml::Table>() else { return vec![] };
    let mut out = Vec::new();
    for h in v.get("hotkeys").and_then(|x| x.as_array()).into_iter().flatten() {
        let Some(b) = h.get("binding").and_then(|x| x.as_str()) else { continue };
        if b.eq_ignore_ascii_case("unassigned") || b.is_empty() {
            continue;
        }
        let id = h.get("id").and_then(|x| x.as_str()).unwrap_or("hotkey");
        let Some(t) = parse_plus_binding(b) else { continue };
        out.push(
            Shortcut::new("OmniWM", &pretty_key_name(id), t, Scope::Global, Origin::ConfigFile)
                .bundle(Some("com.barut.OmniWM"))
                .detail(format!("{} · {id} = \"{b}\"", p.display())),
        );
    }
    out
}

pub fn skhd(extra_paths: &[String]) -> Vec<Shortcut> {
    let h = home();
    let xdg = std::env::var("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or(h.join(".config"));
    let mut paths: Vec<PathBuf> = extra_paths
        .iter()
        .map(|s| PathBuf::from(s.replace("$HOME", &h.to_string_lossy()).replace("$XDG_CONFIG_HOME", &xdg.to_string_lossy())))
        .collect();
    paths.extend([xdg.join("skhd/skhdrc"), h.join(".config/skhd/skhdrc"), h.join(".skhdrc")]);
    let Some(p) = first_existing(&paths) else { return vec![] };
    let Ok(text) = std::fs::read_to_string(&p) else { return vec![] };
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with(':') || l.starts_with('.') {
            continue;
        }
        let Some((lhs, cmd)) = l.split_once(':') else { continue };
        let lhs = lhs.trim();
        // Optional "mode <" prefix.
        let lhs = lhs.rsplit('<').next().unwrap_or(lhs).trim();
        let compact: String = lhs.chars().filter(|c| !c.is_whitespace()).collect();
        let Some(t) = parse_dash_binding(&compact) else { continue };
        out.push(Shortcut::new("skhd", cmd.trim(), t, Scope::Global, Origin::ConfigFile).bundle(Some("skhd")).detail(p.display().to_string()));
    }
    out
}

pub fn karabiner() -> Vec<Shortcut> {
    let p = home().join(".config/karabiner/karabiner.json");
    let Ok(text) = std::fs::read_to_string(&p) else { return vec![] };
    let Ok(j) = serde_json::from_str::<serde_json::Value>(&text) else { return vec![] };
    let mut out = Vec::new();
    let profiles = j.get("profiles").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let Some(profile) = profiles.iter().find(|p| p.get("selected").and_then(|x| x.as_bool()) == Some(true)) else { return out };
    let from_trigger = |from: &serde_json::Value| -> Option<Trigger> {
        let kc = from.get("key_code")?.as_str()?;
        let mut mods = 0;
        for m in from.pointer("/modifiers/mandatory").and_then(|x| x.as_array()).into_iter().flatten() {
            mods |= modifier_word(m.as_str()?)?;
        }
        if let Some(m) = modifier_word(kc) {
            // A modifier remapped by itself.
            return Some(Trigger::ModHold { mods: m | mods });
        }
        // Karabiner names a key by its place on a US keyboard.
        Some(crate::layout::with_active(|layout| by_us_position(mods, normalize_key(kc), layout)))
    };
    for rule in profile.pointer("/complex_modifications/rules").and_then(|x| x.as_array()).into_iter().flatten() {
        let desc = rule.get("description").and_then(|x| x.as_str()).unwrap_or("Karabiner rule");
        for m in rule.get("manipulators").and_then(|x| x.as_array()).into_iter().flatten() {
            if m.get("conditions").is_some_and(|c| c.as_array().is_some_and(|a| !a.is_empty())) {
                // Device-specific rules do not touch the built-in keyboard.
                continue;
            }
            let Some(t) = m.get("from").and_then(from_trigger) else { continue };
            out.push(
                Shortcut::new("Karabiner-Elements", desc, t, Scope::Global, Origin::KeyRemap)
                    .bundle(Some("org.pqrs.Karabiner-Elements.Settings"))
                    .detail(p.display().to_string()),
            );
        }
    }
    for sm in profile.get("simple_modifications").and_then(|x| x.as_array()).into_iter().flatten() {
        let Some(from) = sm.pointer("/from/key_code").and_then(|x| x.as_str()) else { continue };
        let to = sm.pointer("/to/0/key_code").and_then(|x| x.as_str()).unwrap_or("?");
        out.push(
            Shortcut::new("Karabiner-Elements", &format!("Remap {from} → {to}"), physical(Trigger::combo(0, normalize_key(from))), Scope::Global, Origin::KeyRemap)
                .bundle(Some("org.pqrs.Karabiner-Elements.Settings"))
                .detail(p.display().to_string()),
        );
    }
    out
}

fn pretty_action(a: &str) -> String {
    match a {
        "ptt" => "Push to talk (dictate while held)".into(),
        "popo" => "Hands-free dictation".into(),
        "dismiss" => "Dismiss".into(),
        "lens" => "Lens".into(),
        "paste_last_text" => "Paste last dictation".into(),
        "enter_rebind" => "Press Enter after dictation".into(),
        other => pretty_key_name(other),
    }
}

/// Wispr Flow stores shortcuts as key codes joined by "+" (55+59+9 = ⌘⌃V); codes from 4096 are mouse buttons.
pub fn wispr_flow() -> Vec<Shortcut> {
    let p = home().join("Library/Application Support/Wispr Flow/config.json");
    let Ok(text) = std::fs::read_to_string(&p) else { return vec![] };
    let Ok(j) = serde_json::from_str::<serde_json::Value>(&text) else { return vec![] };
    let Some(map) = j.pointer("/prefs/user/shortcuts").and_then(|x| x.as_object()) else { return vec![] };
    let mut out = Vec::new();
    for (codes, action) in map {
        let Some(action) = action.as_str() else { continue };
        let mut mods = 0;
        let mut key: Option<String> = None;
        let mut mouse = false;
        for c in codes.split('+') {
            let Ok(n) = c.trim().parse::<i64>() else { continue };
            if let Some(m) = keycode_modifier(n) {
                mods |= m;
            } else if n >= 4096 {
                key = Some(format!("Mouse button {}", n - 4096 + 1));
                mouse = true;
            } else if let Some(k) = keycode_key(n) {
                key = Some(k);
            }
        }
        let trigger = match key {
            Some(k) => clean_trigger(mods, k),
            None if mods != 0 => Trigger::ModHold { mods },
            None => continue,
        };
        if action == "dismiss" && mods == 0 {
            // Escape to cancel only works while dictating.
            continue;
        }
        out.push(
            Shortcut::new("Wispr Flow", &pretty_action(action), trigger, Scope::Global, Origin::AppSettings)
                .bundle(Some("com.electron.wispr-flow"))
                .detail(format!("config.json › prefs.user.shortcuts \"{codes}\""))
                .estimated(mouse),
        );
    }
    out
}

/// One hotkey found in a JSON settings file: the key path, the stored text and the keys.
#[derive(Debug, PartialEq)]
pub struct JsonHotkey {
    pub path: Vec<String>,
    pub text: String,
    pub trigger: Trigger,
}

/// Find hotkeys in JSON settings. A hotkey is text such as "Alt+Space" below a key whose
/// name says shortcut, hotkey or accelerator (Todoist: `global_shortcuts.quick_add`).
pub fn hotkeys_in_json(v: &serde_json::Value) -> Vec<JsonHotkey> {
    fn walk(v: &serde_json::Value, path: &mut Vec<String>, out: &mut Vec<JsonHotkey>) {
        if path.len() > 8 {
            return;
        }
        match v {
            serde_json::Value::Object(m) => {
                for (k, x) in m {
                    path.push(k.clone());
                    walk(x, path, out);
                    path.pop();
                }
            }
            serde_json::Value::Array(a) => {
                for (i, x) in a.iter().take(64).enumerate() {
                    path.push(i.to_string());
                    walk(x, path, out);
                    path.pop();
                }
            }
            serde_json::Value::String(s) => {
                let named = path.iter().any(|p| {
                    let p = p.to_lowercase();
                    p.contains("shortcut") || p.contains("hotkey") || p.contains("accelerator")
                });
                if !named || s.len() > 40 {
                    return;
                }
                // A key with no modifier types text, except F1 to F20.
                let Some(t) = electron::parse_accelerator(s) else { return };
                let f_key = |k: &str| k.starts_with('F') && k.len() > 1 && k[1..].chars().all(|c| c.is_ascii_digit());
                let usable = match &t {
                    Trigger::Combo { mods, key } => *mods != 0 || f_key(key),
                    _ => false,
                };
                if usable {
                    out.push(JsonHotkey { path: path.clone(), text: s.clone(), trigger: t });
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(v, &mut Vec::new(), &mut out);
    out
}

/// The action name for a hotkey from its key path: "quickSearchShortcut" gives "Quick Search".
fn json_hotkey_label(folder: &str, path: &[String]) -> String {
    const GENERIC: [&str; 6] = ["accelerator", "shortcut", "hotkey", "key", "keys", "value"];
    let leaf = path.iter().rev().find(|p| !GENERIC.contains(&p.to_lowercase().as_str()) && p.parse::<usize>().is_err()).cloned().unwrap_or_default();
    match (folder, leaf.as_str()) {
        ("Todoist", "quick_add") => return "Quick Add (new task)".into(),
        ("Todoist", "activate") => return "Show or hide Todoist".into(),
        ("Todoist", "quick_add_ramble") => return "Quick Add: Ramble".into(),
        _ => {}
    }
    let name = pretty_key_name(&leaf);
    let lower = name.to_lowercase();
    for end in [" shortcut", " hotkey", " accelerator"] {
        if lower.ends_with(end) && name.len() > end.len() {
            return name[..name.len() - end.len()].to_string();
        }
    }
    if name.is_empty() { "Hotkey".into() } else { name }
}

/// True when the key path says the hotkey works from any app.
fn json_path_is_system_wide(path: &[String]) -> bool {
    path.iter().any(|p| {
        let p = p.to_lowercase();
        ["global", "hotkey", "quick", "launch", "activate", "toggle", "summon"].iter().any(|w| p.contains(w))
    })
}

/// Apps (Todoist, Notion, Claude) that keep hotkeys as text in a JSON file in
/// ~/Library/Application Support/<app name>/. Only installed apps are read.
pub fn json_settings_hotkeys(apps: &[AppInfo]) -> Vec<Shortcut> {
    // Wispr Flow has its own reader (key codes, not text).
    const OWN_READER: [&str; 1] = ["Wispr Flow"];
    let root = home().join("Library/Application Support");
    let mut out: Vec<Shortcut> = Vec::new();
    let mut seen: HashSet<(String, String, Trigger)> = HashSet::new();
    for a in apps.iter().filter(|a| !a.name.is_empty() && !OWN_READER.contains(&a.name.as_str())) {
        let Ok(rd) = std::fs::read_dir(root.join(&a.name)) else { continue };
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
        files.sort();
        for f in files {
            if std::fs::metadata(&f).map(|m| m.len() > 1_000_000).unwrap_or(true) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&f) else { continue };
            let Ok(j) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
            let file = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            for h in hotkeys_in_json(&j) {
                let action = json_hotkey_label(&a.name, &h.path);
                if !seen.insert((a.name.clone(), action.clone(), h.trigger.clone())) {
                    continue;
                }
                let wide = json_path_is_system_wide(&h.path);
                let sure = h.path.iter().any(|p| p.to_lowercase().contains("global"));
                out.push(
                    Shortcut::new(&a.name, &action, h.trigger, if wide { Scope::Global } else { Scope::App }, Origin::AppSettings)
                        .bundle(a.bundle.as_deref())
                        .detail(format!("{file} › {} \"{}\"", h.path.join("."), h.text))
                        .estimated(!sure),
                );
            }
        }
    }
    out
}

const SPACEMAN: &str = "anomaly.spaceman";

fn spaceman_named_workspaces(config: Option<&str>, state: Option<&str>) -> BTreeSet<char> {
    let mut letters = BTreeSet::new();
    let mut add = |name: &str| {
        if let [letter] = name.as_bytes() && letter.is_ascii_alphabetic() {
            letters.insert(char::from(letter.to_ascii_uppercase()));
        }
    };
    if let Some(config) = config.and_then(|text| text.parse::<toml::Table>().ok())
        && config.get("version").and_then(toml::Value::as_integer) == Some(1)
    {
        let tables = |key: &str| config.get(key).and_then(toml::Value::as_array).into_iter().flatten();
        let routes = tables("rules").filter(|rule| rule.get("action").and_then(toml::Value::as_str) == Some("route"));
        for name in tables("workspaces").chain(routes).filter_map(|entry| entry.get("workspace")?.as_str()) {
            add(name);
        }
    }
    if let Some(state) = state.and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok()) {
        let entries = |key: &str| state.get(key).and_then(serde_json::Value::as_array).into_iter().flatten();
        for name in ["active", "previous", "floating"].into_iter().flat_map(entries).filter_map(|entry| entry.get(1)?.as_str()) {
            add(name);
        }
        for (letter, slots) in ('A'..='Z').zip(entries("slots").skip(10)) {
            if slots.as_array().is_some_and(|slots| !slots.is_empty()) {
                letters.insert(letter);
            }
        }
    }
    letters
}

fn spaceman_defaults(app: &str, version: &str, named_workspaces: &BTreeSet<char>, layout: Option<&crate::layout::Layout>) -> Vec<Shortcut> {
    if !matches!(version, "0.3.2" | "0.3.3") {
        return vec![];
    }
    let detail = format!("Spaceman {version} hotkey defaults; registration depends on Spaceman's runtime state");
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut collision = false;
    let mut add = |action: &str, mods: u8, key: &str, fixed_position: bool| {
        let physical = at_us_position(Trigger::combo(mods, key), layout);
        let logical = |layout: &crate::layout::Layout| {
            if layout.keys.values().any(|base| base == key) {
                return Some(Trigger::combo(mods, key));
            }
            shifted_base_in(key, Some(layout)).map(|base| Trigger::combo(mods | SHIFT, base))
        };
        let mut trigger = if fixed_position { physical.clone() } else { layout.and_then(logical).unwrap_or_else(|| physical.clone()) };
        if seen.contains(&trigger) {
            collision = true;
            if version == "0.3.3" && matches!(key, "`" | "/" | "-" | "=") {
                trigger = physical;
            }
        }
        if seen.insert(trigger.clone()) {
            out.push(Shortcut::new(app, action, trigger, Scope::Global, Origin::BuiltInDefault)
                .bundle(Some(SPACEMAN)).detail(&detail).estimated(true));
        }
    };
    // Numbered workspaces use physical top-row keys. Named workspaces follow the layout.
    for (index, digit) in "1234567890".chars().enumerate() {
        let workspace = index + 1;
        let key = digit.to_string();
        add(&format!("Switch to workspace {workspace}"), OPT, &key, true);
        add(&format!("Move window to workspace {workspace}"), OPT | SHIFT, &key, true);
    }
    for letter in named_workspaces {
        let key = letter.to_string();
        add(&format!("Switch to workspace {letter}"), OPT, &key, false);
        add(&format!("Move window to workspace {letter}"), OPT | SHIFT, &key, false);
    }
    for (letter, direction) in [('H', "left"), ('J', "down"), ('K', "up"), ('L', "right")] {
        if !named_workspaces.contains(&letter) {
            let key = letter.to_string();
            add(&format!("Focus {direction}"), OPT, &key, false);
            add(&format!("Move tile {direction}"), OPT | SHIFT, &key, false);
        }
    }
    if !named_workspaces.contains(&'F') {
        add("Toggle fullscreen layout", OPT | SHIFT, "F", false);
    }
    if version == "0.3.3" {
        add("Toggle floating window", OPT | SHIFT, "Space", true);
        add("Move workspace to next display", OPT | SHIFT, "Tab", true);
    }
    add("Previous workspace on this display", OPT, "Tab", true);
    for (key, action) in [("`", "Next window in the workspace"), ("/", "Flip the split"), ("-", "Shrink the tile"), ("=", "Grow the tile")] {
        add(action, OPT, key, false);
    }
    if version == "0.3.2" && collision {
        return vec![];
    }
    out
}

pub fn spaceman_hotkeys(apps: &[AppInfo]) -> Option<(Vec<Shortcut>, SourceNote)> {
    let app = apps.iter().filter(|app| app.bundle.as_deref() == Some(SPACEMAN)).min_by_key(|app| (!app.running, &app.path))?;
    let info = read_plist(&Path::new(&app.path).join("Contents/Info.plist"));
    let version = info.as_ref().and_then(plist::Value::as_dictionary)
        .and_then(|info| info.get("CFBundleShortVersionString")).and_then(plist::Value::as_string).unwrap_or("unknown");
    if !matches!(version, "0.3.2" | "0.3.3") {
        return Some((vec![], SourceNote {
            name: "Spaceman hotkeys".into(), status: format!("No shortcut definitions for version {version}"), ..Default::default()
        }));
    }
    let root = std::env::var_os("SPACEMAN_APPLICATION_SUPPORT").map(PathBuf::from)
        .unwrap_or_else(|| home().join("Library/Application Support/Spaceman"));
    let config_path = std::env::var_os("SPACEMAN_CONFIG").map(PathBuf::from).unwrap_or_else(|| root.join("config.toml"));
    let config = std::fs::read_to_string(config_path).ok();
    let state = std::fs::read_to_string(root.join("workspace-state.json")).ok();
    let named = spaceman_named_workspaces(config.as_deref(), state.as_deref());
    let shortcuts = crate::layout::with_active(|layout| spaceman_defaults(&app.name, version, &named, layout));
    let note = SourceNote {
        name: "Spaceman hotkeys".into(), count: shortcuts.len(), ok: !shortcuts.is_empty(),
        status: if shortcuts.is_empty() { format!("Version {version}: this keyboard layout has overlapping Spaceman shortcuts") }
            else { format!("Version {version} defaults and named workspaces") },
    };
    Some((shortcuts, note))
}

/// Hotkeys that apps build in and only store once you change them.
pub fn builtin_app_hotkeys(apps: &[AppInfo]) -> Vec<Shortcut> {
    let mut out = Vec::new();
    // Zethos: "Quick-entry shortcut", "Double-tap a modifier" (strings in its program). The default
    // (double-tap ⌥, from the person) applies until "customKeyboardShortcut" is stored in its settings.
    const ZETHOS: &str = "ai.zethos.Zethos";
    if has_app(apps, ZETHOS) {
        let custom = read_plist(&home().join(format!("Library/Preferences/{ZETHOS}.plist")))
            .and_then(|v| v.as_dictionary().and_then(|d| d.get("customKeyboardShortcut").cloned()));
        let s = match custom {
            None => Shortcut::new(&app_name_for(apps, ZETHOS), "Quick entry: new to-do", Trigger::ModTap { mods: OPT, taps: 2 }, Scope::Global, Origin::BuiltInDefault)
                .detail("Zethos default quick-entry shortcut (double-tap a modifier)")
                .estimated(true),
            Some(v) => {
                let t = hotkey_in_value(&v).unwrap_or(Trigger::ModTap { mods: OPT, taps: 2 });
                Shortcut::new(&app_name_for(apps, ZETHOS), "Quick entry: new to-do", t, Scope::Global, Origin::AppSettings)
                    .detail("Zethos settings key \"customKeyboardShortcut\"")
            }
        };
        out.push(s.bundle(Some(ZETHOS)));
    }
    // This app's own panel, on the key from its settings.
    let key = crate::settings::get().panel_key;
    if let Some(mods) = key.mods() {
        out.push(
            Shortcut::new("Keyclash", "Peek at this app's shortcuts", Trigger::ModTap { mods, taps: 2 }, Scope::Global, Origin::AppSettings)
                .bundle(Some(crate::bundle::BUNDLE_ID))
                .detail(format!("Press {} twice (hold to keep it open while held). Change it in Keyclash › Settings › Peek.", key.symbol())),
        );
    }
    out
}

/// VS Code-style editors keep user key bindings in keybindings.json.
pub fn editor_keybindings(apps: &[AppInfo]) -> Vec<Shortcut> {
    let mut out = Vec::new();
    for (folder, app, bundle) in [
        ("Cursor", "Cursor", "com.todesktop.230313mzl4w4u92"),
        ("Code", "Visual Studio Code", "com.microsoft.VSCode"),
        ("Windsurf", "Windsurf", "com.exafunction.windsurf"),
    ] {
        let p = home().join(format!("Library/Application Support/{folder}/User/keybindings.json"));
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        let clean: String = text.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        let clean = clean.replace(",\n]", "\n]").replace(",\n}", "\n}");
        let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(&clean) else { continue };
        let name = apps.iter().find(|a| a.name == app).map(|a| a.name.clone()).unwrap_or(app.into());
        for b in arr {
            let (Some(k), Some(cmd)) = (b.get("key").and_then(|x| x.as_str()), b.get("command").and_then(|x| x.as_str())) else { continue };
            if cmd.starts_with('-') || k.contains(' ') {
                continue;
            }
            let Some(t) = parse_plus_binding(k) else { continue };
            out.push(Shortcut::new(&name, cmd, t, Scope::App, Origin::ConfigFile).bundle(Some(bundle)).detail(p.display().to_string()));
        }
    }
    out
}

// ---------------------------------------------------------------- menus of apps

pub fn trigger_from_menu_equiv(equiv: &str, mask: Option<i64>) -> Option<Trigger> {
    let (key, shift) = key_from_char(equiv)?;
    // An item with a key equivalent and no stored mask uses Command (AppKit default).
    let mods = mods_from_nsevent(mask.unwrap_or(NS_COMMAND)) | if shift { SHIFT } else { 0 };
    if mods == 0 && key.chars().count() == 1 {
        return None;
    }
    Some(clean_trigger(mods, key))
}

fn menus_of_app(a: &AppInfo) -> Vec<Shortcut> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for n in nib::find_menu_nibs(Path::new(&a.path)) {
        for it in nib::read_menu_items(&n) {
            let Some(t) = trigger_from_menu_equiv(&it.key_equiv, it.mod_mask) else { continue };
            let title = it.title.trim().to_string();
            if !seen.insert((title.clone(), t.clone())) {
                continue;
            }
            let place = if it.menu.is_empty() { String::new() } else { format!("{} › ", it.menu) };
            out.push(
                Shortcut::new(&a.name, &title, t, Scope::App, Origin::MenuFile)
                    .bundle(a.bundle.as_deref())
                    .detail(format!("{place}{title} · {}", n.file_name().unwrap_or_default().to_string_lossy()))
                    .estimated(it.mod_mask.is_none()),
            );
        }
    }
    out
}

pub fn menu_file_shortcuts(apps: &mut [AppInfo]) -> (Vec<Shortcut>, SourceNote) {
    let per_app = par_map(apps, 8, menus_of_app);
    let mut out = Vec::new();
    let mut with = 0;
    for (a, list) in apps.iter_mut().zip(per_app) {
        if !list.is_empty() {
            a.has_menu_file = true;
            with += 1;
        }
        out.extend(list);
    }
    let n = out.len();
    (out, SourceNote { name: "Menu files inside apps".into(), count: n, status: format!("{with} apps ship readable menu files"), ok: true })
}

/// Run `f` over `items` on a few threads, keeping the order.
pub fn par_map<T: Sync, R: Send>(items: &[T], threads: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    if items.is_empty() {
        return Vec::new();
    }
    let size = items.len().div_ceil(threads.max(1));
    std::thread::scope(|sc| {
        let handles: Vec<_> = items.chunks(size).map(|chunk| sc.spawn(|| chunk.iter().map(&f).collect::<Vec<R>>())).collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    })
}

pub fn electron_shortcuts(apps: &[AppInfo]) -> (Vec<Shortcut>, SourceNote) {
    let mut out = Vec::new();
    let mut scanned = 0;
    for a in apps.iter().filter(|a| a.has_electron) {
        let p = Path::new(&a.path).join("Contents/Resources/app.asar");
        let Ok(found) = electron::scan_asar(&p) else { continue };
        scanned += 1;
        for (label, t) in found.menu {
            out.push(Shortcut::new(&a.name, &label, t, Scope::App, Origin::ElectronBundle).bundle(a.bundle.as_deref()).detail("accelerator in app.asar").estimated(true));
        }
        for (label, t) in found.global {
            out.push(Shortcut::new(&a.name, &label, t, Scope::Global, Origin::ElectronBundle).bundle(a.bundle.as_deref()).detail("globalShortcut.register in app.asar").estimated(true));
        }
    }
    let n = out.len();
    (out, SourceNote { name: "Electron app code".into(), count: n, status: format!("{scanned} Electron apps scanned"), ok: true })
}

// ---------------------------------------------------------------- live menus

/// System processes whose menus are never read.
pub const IGNORED: [&str; 7] = [
    "com.apple.SystemUIServer",
    "com.apple.controlcenter",
    "com.apple.notificationcenterui",
    "com.apple.Spotlight",
    "com.apple.UserNotificationCenter",
    "com.apple.accessibility.universalAccessAuthWarn",
    "com.apple.loginwindow",
];

/// Browsers whose history and bookmark menus get very long.
pub const BROWSERS: [&str; 10] = [
    "com.apple.Safari", "com.google.Chrome", "org.chromium.Chromium", "org.mozilla.firefox",
    "com.microsoft.edgemac", "com.operasoftware.Opera", "com.brave.Browser", "com.vivaldi.Vivaldi",
    "org.torproject.torbrowser", "org.waterfoxproject.Waterfox",
];

pub const MAX_LIVE_ITEMS: usize = 4000;

/// Menu items per top-level menu in browsers.
pub const BROWSER_MENU_LIMIT: usize = 200;

/// Running app processes: (pid, bundle path).
pub fn running_apps() -> Vec<(i32, String)> {
    let Ok(o) = Command::new("/bin/ps").args(["-axo", "pid=,comm="]).output() else { return vec![] };
    let text = String::from_utf8_lossy(&o.stdout);
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some((pid, path)) = line.split_once(' ') else { continue };
        let Ok(pid) = pid.parse::<i32>() else { continue };
        let Some(bundle) = app_of_program(path.trim()) else { continue };
        out.push((pid, bundle.to_string()));
    }
    out
}

/// The app bundle that a program file belongs to: "/Applications/X.app/Contents/MacOS/X" gives
/// "/Applications/X.app". None for a program outside an app, and for a helper app nested inside another app.
pub fn app_of_program(path: &str) -> Option<&str> {
    let i = path.find(".app/Contents/MacOS/")?;
    let bundle = &path[..i + 4];
    (!bundle[..bundle.len() - 4].contains(".app/")).then_some(bundle)
}

/// Menu items read live through Accessibility, as shortcuts.
pub fn live_items_to_shortcuts(app: &str, bundle: &str, items: Vec<ax::LiveItem>) -> Vec<Shortcut> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for it in items {
        let key = if let Some(c) = &it.cmd_char {
            key_from_ax_char(c)
        } else {
            it.glyph.and_then(key_from_glyph).map(|g| (g.to_string(), false))
        };
        let key = key.or_else(|| it.virtual_key.and_then(keycode_key).map(|k| (k, false)));
        let Some((k, shift)) = key else { continue };
        let mods = mods_from_ax_menu(it.modifiers.unwrap_or(0)) | if shift { SHIFT } else { 0 };
        let t = clean_trigger(mods, k);
        // An item that sits in two menus is one shortcut.
        if !seen.insert((it.title.trim().to_string(), t.clone())) {
            continue;
        }
        out.push(
            Shortcut::new(app, it.title.trim(), t, Scope::App, Origin::LiveMenu)
                .bundle(Some(bundle))
                .detail(format!("{} › {}", it.path, it.title.trim())),
        );
    }
    out
}

pub fn live_menu_shortcuts(apps: &[AppInfo]) -> (Vec<Shortcut>, Vec<String>, SourceNote) {
    if !ax::is_trusted() {
        return (vec![], vec![], SourceNote {
            name: "Live menus of running apps".into(),
            count: 0,
            status: "Accessibility permission not granted — running apps' live menus skipped".into(),
            ok: false,
        });
    }
    let mut out = Vec::new();
    let mut read_bundles = Vec::new();
    let mut done = HashSet::new();
    for (pid, path) in running_apps() {
        let canon = std::fs::canonicalize(&path).map(|p| p.to_string_lossy().to_string()).unwrap_or(path.clone());
        let Some(app) = apps.iter().find(|a| a.path == canon) else { continue };
        let Some(bundle) = app.bundle.clone() else { continue };
        if IGNORED.contains(&bundle.as_str()) || app.background_only || !done.insert(bundle.clone()) {
            continue;
        }
        let limit = BROWSERS.contains(&bundle.as_str()).then_some(BROWSER_MENU_LIMIT);
        let Some(items) = ax::read_app_menus(pid, MAX_LIVE_ITEMS, limit) else { continue };
        read_bundles.push(bundle.clone());
        out.extend(live_items_to_shortcuts(&app.name, &bundle, items));
    }
    let n = out.len();
    let note = SourceNote {
        name: "Live menus of running apps".into(),
        count: n,
        status: format!("{} running apps read", read_bundles.len()),
        ok: true,
    };
    (out, read_bundles, note)
}

// ---------------------------------------------------------------- memory of menus

pub fn cache_path() -> PathBuf {
    data_dir().join("menus.json")
}

/// Version of the remembered menus. Version 1 (no number in the file) stored a false Shift
/// on every letter key, so those files are not used.
pub const CACHE_VERSION: u32 = 2;

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct MenuCache {
    #[serde(default)]
    pub version: u32,
    pub apps: HashMap<String, (u64, Vec<Shortcut>)>,
    #[serde(default)]
    identities: HashMap<String, MenuIdentity>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct MenuIdentity {
    path: String,
    version: String,
    modified: String,
}

impl MenuIdentity {
    fn at(path: &Path) -> Option<Self> {
        let info = path.join("Contents/Info.plist");
        let plist = read_plist(&info)?;
        let dict = plist.as_dictionary()?;
        let version = ["CFBundleVersion", "CFBundleShortVersionString"].map(|k| {
            dict.get(k).and_then(|v| v.as_string()).unwrap_or("")
        }).join("/");
        let modified = std::fs::metadata(info).ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos().to_string();
        Some(Self { path: path.to_string_lossy().into_owned(), version, modified })
    }
}

impl MenuCache {
    pub fn menus_for(&self, app: &AppInfo) -> Option<(u64, &[Shortcut])> {
        let bundle = app.bundle.as_ref()?;
        if self.identities.get(bundle)? != &MenuIdentity::at(Path::new(&app.path))? {
            return None;
        }
        self.apps.get(bundle).map(|(when, list)| (*when, list.as_slice()))
    }

    pub fn remember(&mut self, app: &AppInfo, when: u64, list: Vec<Shortcut>) {
        let Some(bundle) = &app.bundle else { return };
        self.apps.insert(bundle.clone(), (when, list));
        if let Some(identity) = MenuIdentity::at(Path::new(&app.path)) {
            self.identities.insert(bundle.clone(), identity);
        } else {
            self.identities.remove(bundle);
        }
    }
}

pub fn load_cache() -> MenuCache {
    load_cache_at(&cache_path()).unwrap_or_default()
}

fn load_cache_at(path: &Path) -> std::io::Result<MenuCache> {
    match std::fs::read(path) {
        Ok(b) => {
            let cache: MenuCache = serde_json::from_slice(&b).map_err(std::io::Error::other)?;
            Ok(if cache.version == CACHE_VERSION { cache } else { MenuCache::default() })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(MenuCache::default()),
        Err(e) => Err(e),
    }
}

/// Keep one app's live menus so the app stays listed after it quits.
pub fn remember_menus(bundle: &str, list: Vec<Shortcut>) {
    let apps = installed_apps();
    let Some(app) = apps.iter().find(|a| a.bundle.as_deref() == Some(bundle)) else { return };
    update_menu_cache(|cache| {
        cache.remember(app, now_secs(), list);
    });
}

pub fn update_menu_cache(change: impl FnOnce(&mut MenuCache)) {
    if let Err(e) = update_menu_cache_at(&cache_path(), change) {
        crate::hotkey::log(&format!("Could not save remembered menus: {e}"));
    }
}

fn update_menu_cache_at(path: &Path, change: impl FnOnce(&mut MenuCache)) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let lock = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path.with_extension("lock"))?;
    lock.lock()?;
    let mut cache = load_cache_at(path)?;
    cache.version = CACHE_VERSION;
    change(&mut cache);
    let bytes = serde_json::to_vec(&cache).map_err(std::io::Error::other)?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(temp, path)?;
    Ok(())
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod menu_cache_tests {
    use super::*;

    struct TempCache(PathBuf);

    impl TempCache {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("keyclash-cache-{}-{id}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> PathBuf { self.0.join("menus.json") }

        fn app(&self, version: &str) -> AppInfo {
            let path = self.0.join("Test.app");
            std::fs::create_dir_all(path.join("Contents")).unwrap();
            let mut d = plist::Dictionary::new();
            d.insert("CFBundleVersion".into(), plist::Value::String(version.into()));
            plist::Value::Dictionary(d).to_file_xml(path.join("Contents/Info.plist")).unwrap();
            AppInfo { name: "Test".into(), bundle: Some("test.app".into()), path: path.to_string_lossy().into_owned(), ..Default::default() }
        }
    }

    impl Drop for TempCache {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
    }

    #[test]
    fn concurrent_menu_updates_keep_every_app() {
        let temp = TempCache::new();
        let gate = std::sync::Arc::new(std::sync::Barrier::new(8));
        std::thread::scope(|scope| {
            for i in 0..8 {
                let gate = gate.clone();
                let path = temp.path();
                scope.spawn(move || {
                    gate.wait();
                    update_menu_cache_at(&path, |cache| {
                        std::thread::yield_now();
                        cache.apps.insert(format!("app.{i}"), (i, vec![]));
                    }).unwrap();
                    // An unlocked reader must always see a complete JSON file.
                    let read = load_cache_at(&path).unwrap();
                    assert!(read.apps.contains_key(&format!("app.{i}")));
                });
            }
        });
        assert_eq!(load_cache_at(&temp.path()).unwrap().apps.len(), 8);
    }

    #[test]
    fn unreadable_cache_is_not_replaced_by_an_empty_update() {
        let temp = TempCache::new();
        std::fs::write(temp.path(), b"incomplete JSON").unwrap();
        assert!(update_menu_cache_at(&temp.path(), |_| {}).is_err());
        assert_eq!(std::fs::read(temp.path()).unwrap(), b"incomplete JSON");
    }

    #[test]
    fn only_the_same_installed_app_version_reuses_menus() {
        let temp = TempCache::new();
        let app = temp.app("1");
        let mut cache = MenuCache::default();
        cache.remember(&app, 7, vec![Shortcut::new("Test", "Action", Trigger::combo(CMD, "K"), Scope::App, Origin::LiveMenu)]);
        assert_eq!(cache.menus_for(&app).unwrap().1.len(), 1);
        temp.app("2");
        assert!(cache.menus_for(&app).is_none());
        // Old records remain stored, but cannot override this app version's menu files.
        assert_eq!(cache.apps.len(), 1);
        let absent = AppInfo { path: temp.0.join("Removed.app").to_string_lossy().into_owned(), ..app };
        assert!(cache.menus_for(&absent).is_none());
    }

    #[test]
    fn legacy_menus_need_validation_before_reuse() {
        let temp = TempCache::new();
        let app = temp.app("1");
        let cache: MenuCache = serde_json::from_str(r#"{"version":2,"apps":{"test.app":[7,[]]}}"#).unwrap();
        assert!(cache.menus_for(&app).is_none());
        assert_eq!(cache.apps.len(), 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaceman_imports_versioned_global_hotkeys() {
        let empty = BTreeSet::new();
        let shortcuts = spaceman_defaults("Spaceman", "0.3.2", &empty, None);
        assert_eq!(shortcuts.len(), 34);
        assert!(shortcuts.iter().all(|s| s.owner() == SPACEMAN && s.scope == Scope::Global && s.estimated));
        assert!(shortcuts.iter().any(|s| s.action == "Switch to workspace 10" && s.trigger == Trigger::combo(OPT, "0")));
        assert!(shortcuts.iter().any(|s| s.action == "Previous workspace on this display" && s.trigger == Trigger::combo(OPT, "Tab")));
        let shortcuts = spaceman_defaults("Spaceman", "0.3.3", &empty, None);
        assert_eq!(shortcuts.len(), 36);
        assert!(shortcuts.iter().any(|s| s.action == "Toggle floating window" && s.trigger == Trigger::combo(OPT | SHIFT, "Space")));
        assert!(shortcuts.iter().any(|s| s.action == "Move workspace to next display" && s.trigger == Trigger::combo(OPT | SHIFT, "Tab")));
        assert!(spaceman_defaults("Spaceman", "0.4.0", &empty, None).is_empty());
        assert!(spaceman_hotkeys(&[]).is_none());
    }

    #[test]
    fn spaceman_named_workspaces_override_direction_and_fullscreen_hotkeys() {
        let config = r#"
            version = 1
            [[workspaces]]
            workspace = "h"
            monitor = "main"
            [[rules]]
            action = "route"
            bundle_id = "example.app"
            workspace = "F"
        "#;
        let state = r#"{"active":[["Main", "C"],["Secondary", 6]],"previous":[["Main", "J"]]}"#;
        let named = spaceman_named_workspaces(Some(config), Some(state));
        assert_eq!(named, BTreeSet::from(['C', 'F', 'H', 'J']));
        let shortcuts = spaceman_defaults("Spaceman", "0.3.2", &named, None);
        assert!(shortcuts.iter().any(|s| s.action == "Switch to workspace H" && s.trigger == Trigger::combo(OPT, "H")));
        assert!(shortcuts.iter().any(|s| s.action == "Move window to workspace F" && s.trigger == Trigger::combo(OPT | SHIFT, "F")));
        assert!(shortcuts.iter().all(|s| !matches!(s.action.as_str(), "Focus left" | "Move tile down" | "Toggle fullscreen layout")));
    }

    #[test]
    fn spaceman_uses_the_physical_punctuation_fallback_when_a_chord_overlaps() {
        let layout = crate::layout::Layout {
            id: "test".into(), name: "test".into(),
            keys: HashMap::from([(26, "7".into()), (44, "△".into())]),
            shifted: HashMap::from([(26, "/".into())]),
        };
        let empty = BTreeSet::new();
        assert!(spaceman_defaults("Spaceman", "0.3.2", &empty, Some(&layout)).is_empty());
        let shortcuts = spaceman_defaults("Spaceman", "0.3.3", &empty, Some(&layout));
        assert!(shortcuts.iter().any(|s| s.action == "Move window to workspace 7" && s.trigger == Trigger::combo(OPT | SHIFT, "7")));
        assert!(shortcuts.iter().any(|s| s.action == "Flip the split" && s.trigger == Trigger::combo(OPT, "△")));
    }

    #[test]
    fn disabled_macshot_capture_does_not_block_redo() {
        // MacShot keeps the old capture chord after its settings show "None".
        let mut d = plist::Dictionary::new();
        for (key, value) in [("hotkeyKeyCode", 6), ("hotkeyModifiers", 768), ("hotkeyQuickCaptureKeyCode", 5), ("hotkeyQuickCaptureModifiers", 768)] {
            d.insert(key.into(), plist::Value::Integer(value.into()));
        }
        d.insert("hotkeyDisabled_1".into(), plist::Value::Boolean(true));
        d.insert("hotkeyDisabled_7".into(), plist::Value::Boolean(false));
        let settings = plist::Value::Dictionary(d);
        for domain in ["com.sw33tlie.macshot.macshot", "com.sw33tlie.macshot.offline"] {
            let mut shortcuts = shortcuts_in_domain(&[], domain, &settings, "test.plist");
            let redo = Trigger::combo(CMD | SHIFT, "Z");
            let quick_capture = Trigger::combo(CMD | SHIFT, "G");
            for app in ["Acrobat", "Automator"] {
                shortcuts.push(Shortcut::new(app, "Redo", redo.clone(), Scope::App, Origin::LiveMenu));
            }
            shortcuts.push(Shortcut::new("Finder", "Go to Folder", quick_capture.clone(), Scope::App, Origin::LiveMenu));
            let conflicts = crate::conflicts::find(&shortcuts);
            assert_eq!(conflicts.iter().find(|c| c.trigger == redo).unwrap().kind, crate::conflicts::Kind::Shared);
            assert_eq!(conflicts.iter().find(|c| c.trigger == quick_capture).unwrap().kind, crate::conflicts::Kind::Blocks);
            let capture = shortcuts.iter().find(|s| s.bundle.as_deref() == Some(domain) && s.trigger == redo).unwrap();
            assert_eq!(capture.action, "Capture Area");
            assert!(!capture.enabled);
            assert!(!capture.estimated);
            assert!(capture.detail.contains("turned off in MacShot"));
            let quick = shortcuts.iter().find(|s| s.bundle.as_deref() == Some(domain) && s.trigger == quick_capture).unwrap();
            assert_eq!(quick.action, "Quick Capture");
            assert!(quick.enabled);
            assert!(!quick.estimated);
        }
        // Another app can use the same key names with different slot numbers.
        let other = shortcuts_in_domain(&[], "test.other", &settings, "test.plist");
        assert!(other.iter().all(|s| s.enabled && s.estimated));
    }

    #[test]
    fn hotkeys_stored_as_two_values_are_found() {
        // macshot: key code 5 with Carbon modifiers 768 (Command + Shift). Key code 0 with 0 is an empty slot.
        let mut d = plist::Dictionary::new();
        for (k, n) in [("hotkeyQuickCaptureKeyCode", 5), ("hotkeyQuickCaptureModifiers", 768), ("hotkeyKeyCode", 6), ("hotkeyModifiers", 768), ("hotkeyRecordFullScreenKeyCode", 0), ("hotkeyRecordFullScreenModifiers", 0), ("windowKeyCode", 3)] {
            d.insert(k.to_string(), plist::Value::Integer(n.into()));
        }
        let mut found = paired_hotkeys(&d);
        found.sort();
        assert_eq!(found, vec![("hotkey".to_string(), Trigger::combo(CMD | SHIFT, "Z")), ("hotkeyQuickCapture".to_string(), Trigger::combo(CMD | SHIFT, "G"))]);
        assert_eq!(hotkey_name("hotkeyQuickCapture"), "Quick Capture");
        assert_eq!(hotkey_name("hotkey"), "Main hotkey");
        assert_eq!(hotkey_name("KeyboardShortcuts_toggleLauncher"), "Toggle Launcher");
    }

    #[test]
    fn json_settings_hotkeys_are_found() {
        // Todoist settings.json and Claude claude_desktop_config.json, as stored on disk.
        let j: serde_json::Value = serde_json::from_str(
            r#"{"stay_in_tray": false,
                "global_shortcuts": {"activate": "Control+Cmd+T", "quick_add": "Alt+Space", "off": ""},
                "preferences": {"quickEntryDictationShortcut": {"accelerator": "Cmd+Shift+Alt+Space"}},
                "sendShortcut": "Enter",
                "title": "Cmd+K"}"#,
        )
        .unwrap();
        let found = hotkeys_in_json(&j);
        let keys: Vec<(String, Trigger)> = found.iter().map(|h| (h.path.join("."), h.trigger.clone())).collect();
        assert!(keys.contains(&("global_shortcuts.quick_add".into(), Trigger::combo(OPT, "Space"))));
        assert!(keys.contains(&("global_shortcuts.activate".into(), Trigger::combo(CTRL | CMD, "T"))));
        assert!(keys.contains(&("preferences.quickEntryDictationShortcut.accelerator".into(), Trigger::combo(CMD | SHIFT | OPT, "Space"))));
        // No modifier, an empty value, and a key name that does not say shortcut: all dropped.
        assert_eq!(found.len(), 3);

        let path = |s: &str| s.split('.').map(String::from).collect::<Vec<_>>();
        assert_eq!(json_hotkey_label("Todoist", &path("global_shortcuts.quick_add")), "Quick Add (new task)");
        assert_eq!(json_hotkey_label("Claude", &path("preferences.quickEntryDictationShortcut.accelerator")), "Quick Entry Dictation");
        assert_eq!(json_hotkey_label("Notion", &path("appState.preferences.quickSearchShortcut")), "Quick Search");
        assert!(json_path_is_system_wide(&path("global_shortcuts.quick_add")));
        assert!(json_path_is_system_wide(&path("appState.preferences.quickSearchShortcut")));
        assert!(!json_path_is_system_wide(&path("editor.sendShortcut")));
    }

    #[test]
    fn a_key_named_by_position_follows_the_layout() {
        let keys = [(14, "F"), (4, "M")];
        let l = crate::layout::Layout { id: "t".into(), name: "T".into(), keys: keys.into_iter().map(|(c, k)| (c, k.to_string())).collect(), shifted: HashMap::new() };
        // AeroSpace "alt-e" is the key at the US "E" place, which types F in this layout.
        assert_eq!(at_us_position(Trigger::combo(OPT, "E"), Some(&l)), Trigger::combo(OPT, "F"));
        assert_eq!(at_us_position(Trigger::combo(OPT, "H"), Some(&l)), Trigger::combo(OPT, "M"));
        // Arrow keys and unchanged keys stay.
        assert_eq!(at_us_position(Trigger::combo(OPT, "Left"), Some(&l)), Trigger::combo(OPT, "Left"));
        assert_eq!(at_us_position(Trigger::combo(OPT, "Q"), Some(&l)), Trigger::combo(OPT, "Q"));
        assert_eq!(at_us_position(Trigger::combo(OPT, "E"), None), Trigger::combo(OPT, "E"));
    }

    #[test]
    fn apps_with_one_display_name_get_their_file_names() {
        let app = |name: &str, bundle: &str, path: &str| AppInfo { name: name.into(), bundle: Some(bundle.into()), path: path.into(), ..Default::default() };
        let mut apps = vec![
            app("ChatGPT", "com.openai.codex", "/Applications/ChatGPT.app"),
            app("ChatGPT", "com.openai.chat", "/Applications/ChatGPT Classic.app"),
            app("Safari", "com.apple.Safari", "/Applications/Safari.app"),
            // The same app found twice keeps its display name.
            app("Notes", "com.apple.Notes", "/System/Applications/Notes.app"),
            app("Notes", "com.apple.Notes", "/Applications/Notes copy.app"),
        ];
        use_file_names_for_twins(&mut apps);
        let names: Vec<&str> = apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["ChatGPT", "ChatGPT Classic", "Safari", "Notes", "Notes"]);
    }
    #[test]
    fn dash_bindings() {
        assert_eq!(parse_dash_binding("cmd-ctrl-alt-left"), Some(Trigger::combo(CMD | CTRL | OPT, "Left")));
        assert_eq!(parse_dash_binding("cmd+alt-h"), Some(Trigger::combo(CMD | OPT, "H")));
        assert_eq!(parse_plus_binding("Control+Option+Command+Left Arrow"), Some(Trigger::combo(CMD | CTRL | OPT, "Left")));
        assert_eq!(parse_plus_binding("Option+Tab"), Some(Trigger::combo(OPT, "Tab")));
    }
    #[test]
    fn user_key_equivalents() {
        assert_eq!(parse_user_key_equivalent("^$g"), Some(Trigger::combo(CTRL | SHIFT, "G")));
        assert_eq!(parse_user_key_equivalent("@~t"), Some(Trigger::combo(CMD | OPT, "T")));
    }
    fn dict(pairs: Vec<(&str, plist::Value)>) -> plist::Dictionary {
        pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }

    #[test]
    fn macos_defaults_and_user_settings() {
        let int = |i: i64| plist::Value::Integer(i.into());
        // Table: a group that holds one plain entry and one nested, turned-off group.
        let table = dict(vec![
            ("name", plist::Value::String("DO_NOT_LOCALIZE: Screenshots".into())),
            ("elements", plist::Value::Array(vec![
                plist::Value::Dictionary(dict(vec![("name", plist::Value::String("Save picture".into())), ("key", int(20)), ("modifier", int(1179648)), ("sybmolichotkey", int(28))])),
                plist::Value::Dictionary(dict(vec![("name", plist::Value::String("Spotlight".into())), ("key", int(49)), ("modifier", int(1048576)), ("sybmolichotkey", int(64))])),
                plist::Value::Dictionary(dict(vec![("name", plist::Value::String("Tile Left Half".into())), ("key", int(123)), ("modifier", int(8650752)), ("sybmolichotkey", int(240))])),
                plist::Value::Dictionary(dict(vec![
                    ("name", plist::Value::String("Zoom".into())),
                    ("enabled", plist::Value::Boolean(false)),
                    ("elements", plist::Value::Array(vec![plist::Value::Dictionary(dict(vec![("name", plist::Value::String("Zoom in".into())), ("key", int(24)), ("modifier", int(1572864)), ("sybmolichotkey", int(17))]))])),
                ])),
            ])),
        ]);
        let mut defs = HashMap::new();
        collect_system_defs(&table, "", true, &mut defs);
        assert!(!defs[&17].default_on, "a nested entry takes \"enabled\" from its group");
        // The person turned Spotlight off and moved nothing else.
        let user = dict(vec![("64", plist::Value::Dictionary(dict(vec![("enabled", plist::Value::Boolean(false))])))]);
        let list = system_shortcuts_from(&defs, &user);
        let keys: Vec<&Trigger> = list.iter().map(|s| &s.trigger).collect();
        assert_eq!(keys, vec![&Trigger::combo(CMD | SHIFT, "3"), &Trigger::combo(CTRL | FN, "Left")]);
        assert!(list.iter().all(|s| s.estimated));
    }

    #[test]
    fn keys_named_by_us_place_ignore_the_shift_rule_of_the_layout() {
        // German: the key at the US "/" place types "-", and "/" is Shift with "7".
        let map = |pairs: &[(i64, &str)]| pairs.iter().map(|&(c, k)| (c, k.to_string())).collect();
        let de = crate::layout::Layout { id: "test.de".into(), name: "German".into(), keys: map(&[(44, "-"), (26, "7"), (30, "+")]), shifted: map(&[(26, "/"), (30, "*")]) };
        let (mods, key) = dash_parts("alt-/").unwrap();
        assert_eq!(by_us_position(mods, key, Some(&de)), Trigger::combo(OPT, "-"), "AeroSpace alt-slash is the key right of the period");
        let (mods, key) = dash_parts("alt-]").unwrap();
        assert_eq!(by_us_position(mods, key, Some(&de)), Trigger::combo(OPT, "+"));
        // A US shifted name keeps Shift on the US base key's place.
        assert_eq!(by_us_position(OPT, "?".into(), Some(&de)), Trigger::combo(OPT | SHIFT, "-"));
    }

    #[test]
    fn a_damaged_data_file_is_kept_aside() {
        let dir = std::env::temp_dir().join(format!("keyclash-test-{}-{}", std::process::id(), now_secs()));
        let file = dir.join("favorites.json");
        write_atomic(&file, b"[\"a\"]").unwrap();
        assert_eq!(read_json::<Vec<String>>(&file), Some(vec!["a".to_string()]));
        std::fs::write(&file, b"[\"a\", trunc").unwrap();
        assert_eq!(read_json::<Vec<String>>(&file), None);
        assert!(!file.exists(), "the damaged file moves aside");
        let kept: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(kept.iter().any(|n| n.starts_with("favorites.json.bad-")), "{kept:?}");
        assert_eq!(read_json::<Vec<String>>(&dir.join("absent.json")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn system_hotkeys_use_the_typed_character() {
        let int = |i: i64| plist::Value::Integer(i.into());
        let entry = |name: &str, key: i64, ch: i64, id: i64| {
            plist::Value::Dictionary(dict(vec![
                ("name", plist::Value::String(name.into())),
                ("key", int(key)),
                ("modifier", int(1048576)),
                ("sybmolichotkey", int(id)),
                ("charKey", int(ch)),
            ]))
        };
        // Key code 4 is H on a US keyboard. On Colemak-DH, M sits there. The menu item matches "m".
        let windows = dict(vec![("name", plist::Value::String("DO_NOT_LOCALIZE: Windows".into())), ("elements", plist::Value::Array(vec![entry("Minimize", 4, 109, 233)]))]);
        // Other groups follow the character too: key code 2 is D on a US keyboard, but this entry types "s".
        // Key code 126 (Up arrow) has no character (65535), so the key code decides.
        let dock = dict(vec![
            ("name", plist::Value::String("DO_NOT_LOCALIZE: Dock".into())),
            ("elements", plist::Value::Array(vec![entry("Dock hiding", 2, 115, 52), entry("Mission Control", 126, 65535, 32)])),
        ]);
        let mut defs = HashMap::new();
        collect_system_defs(&windows, "", true, &mut defs);
        collect_system_defs(&dock, "", true, &mut defs);
        let list = system_shortcuts_from(&defs, &plist::Dictionary::new());
        let keys: Vec<(&str, &Trigger)> = list.iter().map(|s| (s.action.as_str(), &s.trigger)).collect();
        assert_eq!(
            keys,
            vec![("Mission Control", &Trigger::combo(CMD, "Up")), ("Dock hiding", &Trigger::combo(CMD, "S")), ("Minimize", &Trigger::combo(CMD, "M"))]
        );
        // A stored entry gives the character as its first parameter.
        let stored = dict(vec![(
            "233",
            plist::Value::Dictionary(dict(vec![
                ("enabled", plist::Value::Boolean(true)),
                ("value", plist::Value::Dictionary(dict(vec![("type", plist::Value::String("standard".into())), ("parameters", plist::Value::Array(vec![int(110), int(4), int(1048576)]))]))),
            ])),
        )]);
        let list = system_shortcuts_from(&defs, &stored);
        assert!(list.iter().any(|s| s.action == "Minimize" && s.trigger == Trigger::combo(CMD, "N")));
    }

    #[test]
    fn maccy_window_keys_are_not_system_wide() {
        assert!(window_only_key("org.p0deje.Maccy", "KeyboardShortcuts_pin"));
        assert!(!window_only_key("org.p0deje.Maccy", "KeyboardShortcuts_popup"));
        assert!(!window_only_key("com.knollsoft.Rectangle", "KeyboardShortcuts_pin"));
    }

    #[test]
    fn live_menu_letters() {
        let item = |c: &str, m: i64| ax::LiveItem { path: "Edit".into(), title: "x".into(), cmd_char: Some(c.into()), modifiers: Some(m), virtual_key: None, glyph: None };
        let list = live_items_to_shortcuts("App", "b", vec![item("A", 0), item("Z", 1), item("?", 0), item("🎤", 8)]);
        let keys: Vec<Trigger> = list.into_iter().map(|s| s.trigger).collect();
        assert_eq!(keys, vec![Trigger::combo(CMD, "A"), Trigger::combo(CMD | SHIFT, "Z"), Trigger::combo(CMD | SHIFT, "/")]);
    }

    #[test]
    fn menu_equiv() {
        assert_eq!(trigger_from_menu_equiv("Z", Some(1048576)), Some(Trigger::combo(CMD | SHIFT, "Z")));
        assert_eq!(trigger_from_menu_equiv("h", Some(1572864)), Some(Trigger::combo(CMD | OPT, "H")));
    }
}
