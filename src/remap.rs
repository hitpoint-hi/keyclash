//! Change the keys of one shortcut: plan the change, write it with a backup, and undo it.
//!
//! Keyclash writes only where it can prove the result:
//! - AeroSpace and OmniWM config files (TOML): the file must parse after the change, and the parsed
//!   file must equal the old one with only this binding changed.
//! - skhd config: exactly one line changes, and that line reads back as the new keys.
//! - App menu items: macOS "App Shortcuts" (`NSUserKeyEquivalents`), written with `defaults` and read back.
//!
//! Everything else (macOS system hotkeys, Karabiner, settings inside apps) gets the manual steps instead.

use crate::keys::{trigger_text, typed_text};
use crate::sources::{parse_user_key_equivalent, parse_dash_binding as parse_dash, parse_dash_position, parse_plus_binding as parse_plus};
use crate::model::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

pub const AEROSPACE: &str = "bobko.aerospace";
pub const OMNIWM: &str = "com.barut.OmniWM";
pub const SKHD: &str = "skhd";

/// Where and how Keyclash can change one shortcut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Writer {
    Aerospace { path: PathBuf, mode: String, key: String },
    Omniwm { path: PathBuf, id: String },
    Skhd { path: PathBuf },
    MenuKeys { domain: String, title: String },
}

impl Writer {
    pub fn label(&self) -> &'static str {
        match self {
            Writer::Aerospace { .. } => "AeroSpace config file",
            Writer::Omniwm { .. } => "OmniWM config file",
            Writer::Skhd { .. } => "skhd config file",
            Writer::MenuKeys { .. } => "macOS App Shortcuts (NSUserKeyEquivalents)",
        }
    }
}

/// The writer for a shortcut, or the reason that Keyclash cannot change it.
pub fn writer_for(s: &Shortcut) -> Result<Writer, String> {
    if !s.enabled || s.estimated {
        return Err("Check the actual binding before changing a disabled or estimated shortcut.".into());
    }
    let bundle = s.bundle.as_deref().unwrap_or("");
    let (file, rest) = s.detail.split_once(" · ").unwrap_or((s.detail.as_str(), ""));
    match (s.origin, bundle) {
        (Origin::ConfigFile, AEROSPACE) => {
            let inner = rest.strip_prefix("[mode.").and_then(|r| r.split_once(".binding] "));
            match inner {
                Some((mode, key)) if file.starts_with('/') => {
                    Ok(Writer::Aerospace { path: file.into(), mode: mode.to_string(), key: key.to_string() })
                }
                _ => Err("Keyclash cannot find this binding in the AeroSpace file.".into()),
            }
        }
        (Origin::ConfigFile, OMNIWM) => match rest.split_once(" = ") {
            Some((id, _)) if file.starts_with('/') => Ok(Writer::Omniwm { path: file.into(), id: id.to_string() }),
            _ => Err("Keyclash cannot find this hotkey in the OmniWM file.".into()),
        },
        (Origin::ConfigFile, SKHD) if s.detail.starts_with('/') => Ok(Writer::Skhd { path: s.detail.clone().into() }),
        (Origin::LiveMenu | Origin::RememberedMenu | Origin::MenuFile, b) if s.scope == Scope::App && b.contains('.') => {
            Ok(Writer::MenuKeys { domain: b.to_string(), title: s.action.clone() })
        }
        (Origin::AppSettings, b) if s.detail.starts_with("Custom menu shortcut") && !b.is_empty() => {
            let domain = if b == "*" { "NSGlobalDomain".to_string() } else { b.to_string() };
            Ok(Writer::MenuKeys { domain, title: s.action.clone() })
        }
        _ => Err(format!("Keyclash cannot change this shortcut itself. {}", crate::fix::fix_for(s).how)),
    }
}

// ------------------------------------------------------------------ key spelling per tool

fn combo(t: &Trigger) -> Result<(u8, &str), String> {
    match t {
        Trigger::Combo { mods, key } => Ok((*mods, key.as_str())),
        _ => Err("This tool needs a key with modifier keys, not a modifier key by itself.".into()),
    }
}

/// AeroSpace and skhd spell keys in lower case: "left", "pageup".
fn word_key(key: &str) -> String {
    key.to_lowercase().replace(' ', "")
}

fn known_key(key: &str) -> bool {
    crate::layout::us_code(key).is_some()
        || ["Return", "Escape", "Delete", "Space", "Tab", "Left", "Right", "Up", "Down", "Home", "End", "Page Up", "Page Down"].contains(&key)
        || key.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| (1..=20).contains(&n))
}

/// AeroSpace: "cmd-alt-shift-h". Keys are named by their place on a US keyboard (the qwerty preset).
pub fn aerospace_binding(t: &Trigger, by_position: bool) -> Result<String, String> {
    let (mods, key) = combo(t)?;
    if mods & FN != 0 {
        return Err("AeroSpace has no fn modifier.".into());
    }
    let key = if by_position { crate::layout::with_active(|l| crate::layout::us_position_of(key, l)) } else { key.to_string() };
    if !known_key(&key) {
        return Err("Keyclash cannot write this key name for AeroSpace. Pick a standard key.".into());
    }
    let name = match key.as_str() {
        "-" => "minus".into(),
        "=" => "equal".into(),
        "." => "period".into(),
        "," => "comma".into(),
        "/" => "slash".into(),
        "\\" => "backslash".into(),
        "'" => "quote".into(),
        ";" => "semicolon".into(),
        "`" => "backtick".into(),
        "[" => "leftSquareBracket".into(),
        "]" => "rightSquareBracket".into(),
        "Return" => "enter".into(),
        "Escape" => "esc".into(),
        "Delete" => "backspace".into(),
        "Page Up" => "pageUp".into(),
        "Page Down" => "pageDown".into(),
        k => word_key(k),
    };
    let mut parts: Vec<&str> = [(CMD, "cmd"), (OPT, "alt"), (CTRL, "ctrl"), (SHIFT, "shift")].iter().filter(|(b, _)| mods & b != 0).map(|(_, w)| *w).collect();
    parts.push(&name);
    Ok(parts.join("-"))
}

/// OmniWM: "Control+Option+Shift+Command+Left Arrow".
pub fn omniwm_binding(t: &Trigger) -> Result<String, String> {
    let (mods, key) = combo(t)?;
    if mods & FN != 0 {
        return Err("OmniWM bindings have no fn modifier.".into());
    }
    let name = match key {
        "Left" | "Right" | "Up" | "Down" => format!("{key} Arrow"),
        k => k.to_string(),
    };
    let mut parts: Vec<String> = [(CTRL, "Control"), (OPT, "Option"), (SHIFT, "Shift"), (CMD, "Command")].iter().filter(|(b, _)| mods & b != 0).map(|(_, w)| w.to_string()).collect();
    parts.push(name);
    Ok(parts.join("+"))
}

/// skhd: "ctrl + alt - h". skhd names punctuation by key code only, so those keys are refused.
pub fn skhd_binding(t: &Trigger) -> Result<String, String> {
    let (mods, key) = combo(t)?;
    if !known_key(key) {
        return Err("Keyclash cannot write this key name for skhd. Pick a standard key.".into());
    }
    if key.chars().count() == 1 && !key.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("skhd names punctuation keys by key code. Pick a letter, digit, or named key.".into());
    }
    let name = match key {
        "Page Up" => "pageup".into(),
        "Page Down" => "pagedown".into(),
        "Delete" => "backspace".into(),
        k => word_key(k),
    };
    let m: Vec<&str> = [(FN, "fn"), (CMD, "cmd"), (OPT, "alt"), (CTRL, "ctrl"), (SHIFT, "shift")].iter().filter(|(b, _)| mods & b != 0).map(|(_, w)| *w).collect();
    Ok(if m.is_empty() { name } else { format!("{} - {name}", m.join(" + ")) })
}

/// App Shortcuts: "@~$^k". Arrows and function keys use the AppKit private-use characters.
pub fn menu_equivalent(t: &Trigger) -> Result<String, String> {
    let (mods, key) = combo(t)?;
    if mods & FN != 0 {
        return Err("App Shortcuts cannot use fn.".into());
    }
    if mods & (CMD | OPT | CTRL) == 0 {
        return Err("A menu shortcut needs ⌘, ⌥ or ⌃.".into());
    }
    let ch: String = match key {
        "Up" => "\u{F700}".into(),
        "Down" => "\u{F701}".into(),
        "Left" => "\u{F702}".into(),
        "Right" => "\u{F703}".into(),
        "Home" => "\u{F729}".into(),
        "End" => "\u{F72B}".into(),
        "Page Up" => "\u{F72C}".into(),
        "Page Down" => "\u{F72D}".into(),
        "Forward Delete" => "\u{F728}".into(),
        k if k.starts_with('F') && k.len() > 1 && k[1..].chars().all(|c| c.is_ascii_digit()) => {
            let n: u32 = k[1..].parse().map_err(|_| "Unknown function key.".to_string())?;
            if !(1..=20).contains(&n) {
                return Err("Unknown function key.".into());
            }
            char::from_u32(0xF704 + n - 1).map(String::from).ok_or("Unknown function key.")?
        }
        k if k.chars().count() == 1 => k.to_lowercase(),
        _ => return Err(format!("App Shortcuts cannot use the {key} key.")),
    };
    let mut out = String::new();
    for (bit, c) in [(CTRL, '^'), (OPT, '~'), (SHIFT, '$'), (CMD, '@')] {
        if mods & bit != 0 {
            out.push(c);
        }
    }
    out.push_str(&ch);
    Ok(out)
}

// ------------------------------------------------------------------ parsing back (proof)

/// The keys an AeroSpace binding names, in the layout in use.
fn read_aerospace(binding: &str, by_position: bool) -> Option<Trigger> {
    if by_position { parse_dash_position(binding) } else { parse_dash(binding) }
}

// ------------------------------------------------------------------ text edits

/// Change one AeroSpace binding key in `[mode.<mode>.binding]`.
pub fn edit_aerospace(text: &str, mode: &str, old_key: &str, new_key: &str) -> Result<String, String> {
    let header = format!("[mode.{mode}.binding]");
    let mut in_section = false;
    let mut hits = Vec::new();
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.starts_with('[') {
            in_section = t.split('#').next().unwrap_or("").trim().replace(' ', "") == header;
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((k, _)) = t.split_once('=') else { continue };
        let k = k.trim();
        let bare = k.trim_matches('"').trim_matches('\'');
        if bare == old_key {
            hits.push((i, k.to_string()));
        }
        if bare == new_key {
            return Err(format!("AeroSpace already has {new_key} in mode \"{mode}\". Change or remove that binding first."));
        }
    }
    let [(i, written)] = hits.as_slice() else {
        return Err(format!("Keyclash found {} lines for {old_key} under {header}. It changes a file only when it finds exactly one. Is the binding written as a dotted key or an inline table?", hits.len()));
    };
    let quoted = if new_key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') { new_key.to_string() } else { format!("\"{new_key}\"") };
    let pos = lines[*i].find(written.as_str()).ok_or("The binding line changed while Keyclash read it.")?;
    let mut out = String::with_capacity(text.len() + 8);
    for (j, line) in lines.iter().enumerate() {
        if j == *i {
            out.push_str(&line[..pos]);
            out.push_str(&quoted);
            out.push_str(&line[pos + written.len()..]);
        } else {
            out.push_str(line);
        }
    }
    Ok(out)
}

/// Proof for an AeroSpace edit: the new file equals the old file with only this key renamed.
pub fn check_aerospace(before: &str, after: &str, mode: &str, old_key: &str, new_key: &str) -> Result<(), String> {
    let mut want: toml::Table = before.parse().map_err(|e| format!("The AeroSpace file does not parse now: {e}"))?;
    let got: toml::Table = after.parse().map_err(|e| format!("The changed AeroSpace file does not parse: {e}"))?;
    let b = want
        .get_mut("mode")
        .and_then(|m| m.get_mut(mode))
        .and_then(|m| m.get_mut("binding"))
        .and_then(|b| b.as_table_mut())
        .ok_or("The mode is missing in the AeroSpace file.")?;
    let v = b.remove(old_key).ok_or("The binding is missing in the AeroSpace file.")?;
    b.insert(new_key.to_string(), v);
    if want != got {
        return Err("The changed AeroSpace file differs in more than this binding. Keyclash did not write it.".into());
    }
    Ok(())
}

fn id_line(line: &str, id: &str) -> bool {
    let re = format!(r#"(^|[{{,\s])id\s*=\s*"{}""#, regex::escape(id));
    regex::Regex::new(&re).is_ok_and(|r| r.is_match(line))
}

/// Change the binding of one OmniWM hotkey (`[[hotkeys]]` with `id = "<id>"`).
pub fn edit_omniwm(text: &str, id: &str, old: &str, new: &str) -> Result<String, String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let binding = regex::Regex::new(&format!(r#"binding\s*=\s*"{}""#, regex::escape(old))).map_err(|e| e.to_string())?;
    // Blocks start at a table header. The hotkey's block is the one with its id line.
    let mut starts: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| l.trim_start().starts_with('[')).map(|(i, _)| i).collect();
    starts.push(lines.len());
    let mut target = None;
    for (i, line) in lines.iter().enumerate() {
        if !id_line(line, id) {
            continue;
        }
        if binding.is_match(line) {
            // An inline table: the id and the binding are on one line. Only one binding may be there.
            if binding.find_iter(line).count() != 1 || line.matches("id").count() > 1 {
                return Err("This OmniWM file keeps several hotkeys on one line. Keyclash does not change such a line.".into());
            }
            target = Some(i);
            break;
        }
        let from = starts.iter().rev().find(|&&s| s <= i).copied().unwrap_or(0);
        let to = starts.iter().find(|&&s| s > i).copied().unwrap_or(lines.len());
        let found: Vec<usize> = (from..to).filter(|&j| binding.is_match(lines[j])).collect();
        if let [j] = found.as_slice() {
            target = Some(*j);
            break;
        }
    }
    let i = target.ok_or_else(|| format!("Keyclash cannot find binding = \"{old}\" for the OmniWM hotkey {id}."))?;
    let mut out = String::new();
    for (j, line) in lines.iter().enumerate() {
        if j == i {
            out.push_str(&binding.replacen(line, 1, regex::NoExpand(&format!("binding = \"{new}\""))));
        } else {
            out.push_str(line);
        }
    }
    Ok(out)
}

/// Proof for an OmniWM edit: the new file equals the old file with only this hotkey's binding changed.
pub fn check_omniwm(before: &str, after: &str, id: &str, new: &str) -> Result<(), String> {
    let mut want: toml::Table = before.parse().map_err(|e| format!("The OmniWM file does not parse now: {e}"))?;
    let got: toml::Table = after.parse().map_err(|e| format!("The changed OmniWM file does not parse: {e}"))?;
    let list = want.get_mut("hotkeys").and_then(|h| h.as_array_mut()).ok_or("The OmniWM file has no hotkeys.")?;
    let mut n = 0;
    for h in list.iter_mut() {
        if h.get("id").and_then(|x| x.as_str()) == Some(id)
            && let Some(t) = h.as_table_mut() {
                t.insert("binding".into(), toml::Value::String(new.to_string()));
                n += 1;
        }
    }
    if n != 1 || want != got {
        return Err("The changed OmniWM file differs in more than this hotkey. Keyclash did not write it.".into());
    }
    Ok(())
}

/// The left side of an skhd line ("ctrl + alt - h"), without a mode prefix, and where it sits.
fn skhd_lhs(line: &str) -> Option<(usize, usize)> {
    let t = line.trim_start();
    if t.is_empty() || t.starts_with('#') || t.starts_with(':') || t.starts_with('.') {
        return None;
    }
    let colon = line.find(':')?;
    let start = line[..colon].rfind('<').map(|p| p + 1).unwrap_or(0);
    let seg = &line[start..colon];
    let lead = seg.len() - seg.trim_start().len();
    let s = start + lead;
    let e = start + seg.trim_end().len();
    (e > s).then_some((s, e))
}

fn skhd_trigger(lhs: &str) -> Option<Trigger> {
    let compact: String = lhs.chars().filter(|c| !c.is_whitespace()).collect();
    parse_dash(&compact)
}

/// Change the keys of the one skhd line with these keys and this command.
pub fn edit_skhd(text: &str, old: &Trigger, command: &str, new_lhs: &str) -> Result<String, String> {
    if text.lines().any(|line| line.trim_end().ends_with('\\')) {
        return Err("This skhd file uses line continuations. Edit it manually and scan again.".into());
    }
    let new_trigger = skhd_trigger(new_lhs).ok_or("Cannot parse the new skhd binding.")?;
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut hits = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some((s, e)) = skhd_lhs(line) else { continue };
        if skhd_trigger(&line[s..e]).as_ref() == Some(&new_trigger) && &new_trigger != old {
            return Err("skhd already uses these keys. Change that binding first.".into());
        }
        let colon = line.find(':').unwrap_or(line.len());
        if skhd_trigger(&line[s..e]).as_ref() == Some(old) && line[colon + 1..].trim() == command.trim() {
            hits.push((i, s, e));
        }
    }
    let [(i, s, e)] = hits.as_slice() else {
        return Err(format!("Keyclash found {} skhd lines with these keys and command. It changes a file only when it finds exactly one.", hits.len()));
    };
    let mut out = String::new();
    for (j, line) in lines.iter().enumerate() {
        if j == *i {
            out.push_str(&line[..*s]);
            out.push_str(new_lhs);
            out.push_str(&line[*e..]);
        } else {
            out.push_str(line);
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------ plans

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Change {
    File { path: String, before: String, after: String },
    MenuKeys { domain: String, title: String, before: Option<BTreeMap<String, String>>, after: BTreeMap<String, String> },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub plan_id: String,
    pub shortcut_id: String,
    pub app: String,
    pub action: String,
    pub old_keys: String,
    pub new_keys: String,
    pub new_typed: String,
    pub writer: String,
    pub change: Change,
    /// What the person or agent does after the write so the tool sees it.
    pub after_apply: String,
    pub created: u64,
}

impl Plan {
    /// The changed lines, as "- old" and "+ new".
    pub fn diff(&self) -> String {
        match &self.change {
            Change::File { path, before, after } => {
                let mut out = format!("--- {path}\n+++ {path}\n");
                for (n, (a, b)) in before.lines().zip(after.lines()).enumerate() {
                    if a != b {
                        out.push_str(&format!("@@ line {} @@\n- {a}\n+ {b}\n", n + 1));
                    }
                }
                out
            }
            Change::MenuKeys { domain, title, before, after } => {
                let old = before.as_ref().and_then(|d| d.get(title)).map(|s| format!("\"{title}\" = \"{}\"", s.escape_unicode())).unwrap_or_else(|| format!("(no App Shortcut for \"{title}\")"));
                format!("defaults {domain} NSUserKeyEquivalents\n- {old}\n+ \"{title}\" = \"{}\"\n", after.get(title).map(|s| s.escape_unicode().to_string()).unwrap_or_default())
            }
        }
    }
}

fn fnv(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// A short stable id for a text.
pub fn short_id(prefix: &str, text: &str) -> String {
    format!("{prefix}{:010x}", fnv(text) & 0xff_ffff_ffff)
}

/// Make a plan. Nothing is written.
pub fn propose(s: &Shortcut, shortcut_id: &str, new: &Trigger) -> Result<Plan, String> {
    if !s.enabled || s.estimated {
        return Err("This shortcut is disabled or estimated. Check the actual binding before changing it.".into());
    }
    if &s.trigger == new {
        return Err("The new keys are the same as the old keys.".into());
    }
    let w = writer_for(s)?;
    let (change, after_apply) = match &w {
        Writer::Aerospace { path, mode, key } => {
            let before = read_text(path)?;
            let by_position = aerospace_by_position(&before);
            let parsed: toml::Table = before.parse().map_err(|e| format!("Cannot parse the AeroSpace file: {e}"))?;
            if !by_position {
                return Err("This AeroSpace file uses a non-QWERTY preset. Edit it manually and scan again.".into());
            }
            if parsed.get("key-mapping").and_then(|v| v.get("key-notation-to-key-code" )).is_some() {
                return Err("This AeroSpace file has custom key mappings. Edit it manually and scan again.".into());
            }
            let action = parsed.get("mode").and_then(|v| v.get(mode)).and_then(|v| v.get("binding")).and_then(|v| v.get(key));
            let action = action.and_then(|v| match v {
                toml::Value::String(s) => Some(s.clone()),
                toml::Value::Array(a) => Some(a.iter().filter_map(toml::Value::as_str).collect::<Vec<_>>().join("; ")),
                _ => None,
            });
            if action.as_deref() != Some(s.action.as_str()) || read_aerospace(key, by_position).as_ref() != Some(&s.trigger) {
                return Err("The AeroSpace binding changed since the scan. Scan again before proposing a change.".into());
            }
            let new_key = aerospace_binding(new, by_position)?;
            if read_aerospace(&new_key, by_position).as_ref() != Some(new) {
                return Err(format!("Keyclash cannot write these keys for AeroSpace (it wrote \"{new_key}\", which reads back as other keys)."));
            }
            if let Some(dup) = aerospace_keys(&before, mode).into_iter().find(|k| k != key && read_aerospace(k, by_position).as_ref() == Some(new)) {
                return Err(format!("AeroSpace already has {dup} in mode \"{mode}\", which is the same keys. Change or remove that binding first."));
            }
            let after = edit_aerospace(&before, mode, key, &new_key)?;
            check_aerospace(&before, &after, mode, key, &new_key)?;
            (Change::File { path: path.display().to_string(), before, after }, "Run `aerospace reload-config` so AeroSpace uses the new keys.".to_string())
        }
        Writer::Omniwm { path, id } => {
            let before = read_text(path)?;
            let old = s.detail.split_once(" = \"").map(|(_, v)| v.trim_end_matches('"')).ok_or("Keyclash cannot read the old OmniWM binding.")?;
            let new_b = omniwm_binding(new)?;
            let parsed: toml::Table = before.parse().map_err(|e| format!("Cannot parse the OmniWM file: {e}"))?;
            if parsed.get("hotkeys").and_then(toml::Value::as_array).is_some_and(|rows| rows.iter().any(|h| {
                h.get("id").and_then(toml::Value::as_str) != Some(id.as_str()) && h.get("binding").and_then(toml::Value::as_str).and_then(parse_plus).as_ref() == Some(new)
            })) {
                return Err("OmniWM already uses these keys. Change that binding first.".into());
            }
            if parse_plus(&new_b).as_ref() != Some(new) {
                return Err(format!("Keyclash cannot write these keys for OmniWM (\"{new_b}\" reads back as other keys)."));
            }
            let after = edit_omniwm(&before, id, old, &new_b)?;
            check_omniwm(&before, &after, id, &new_b)?;
            (Change::File { path: path.display().to_string(), before, after }, "OmniWM reads its settings file again when it changes. If the old keys still work, quit and open OmniWM.".to_string())
        }
        Writer::Skhd { path } => {
            let before = read_text(path)?;
            let lhs = skhd_binding(new)?;
            if skhd_trigger(&lhs).as_ref() != Some(new) {
                return Err(format!("Keyclash cannot write these keys for skhd (\"{lhs}\" reads back as other keys)."));
            }
            let after = edit_skhd(&before, &s.trigger, &s.action, &lhs)?;
            (Change::File { path: path.display().to_string(), before, after }, "Run `skhd --reload` (or restart skhd) so it uses the new keys.".to_string())
        }
        Writer::MenuKeys { domain, title } => {
            if crate::sources::home().join("Library/Containers").join(domain).exists() {
                return Err("This app has sandboxed preferences. Change its App Shortcuts in System Settings, then scan again.".into());
            }
            let equiv = menu_equivalent(new)?;
            if parse_user_key_equivalent(&equiv).as_ref() != Some(new) {
                return Err("Keyclash cannot write these keys as an App Shortcut.".into());
            }
            let before = read_menu_keys(domain)?;
            let after = changed_menu_keys(before.as_ref(), title, equiv)?;
            let app = if domain == "NSGlobalDomain" { "each app".to_string() } else { s.app.clone() };
            (
                Change::MenuKeys { domain: domain.clone(), title: title.clone(), before, after },
                format!("Quit and open {app} again. macOS applies App Shortcuts when an app starts. The menu title must match exactly; if two menus have an item named \"{title}\", both get the new keys."),
            )
        }
    };
    let id_text = format!("{shortcut_id}|{}|{:?}", trigger_text(new), change);
    Ok(Plan {
        plan_id: short_id("p", &id_text),
        shortcut_id: shortcut_id.to_string(),
        app: s.app.clone(),
        action: s.action.clone(),
        old_keys: trigger_text(&s.trigger),
        new_keys: trigger_text(new),
        new_typed: typed_text(new),
        writer: w.label().to_string(),
        change,
        after_apply,
        created: crate::sources::now_secs(),
    })
}

fn read_text(p: &Path) -> Result<String, String> {
    std::fs::read_to_string(p).map_err(|e| format!("Keyclash cannot read {}: {e}", p.display()))
}

/// The binding keys of one AeroSpace mode, as written.
fn aerospace_keys(text: &str, mode: &str) -> Vec<String> {
    text.parse::<toml::Table>()
        .ok()
        .and_then(|v| Some(v.get("mode")?.get(mode)?.get("binding")?.as_table()?.keys().cloned().collect()))
        .unwrap_or_default()
}

fn aerospace_by_position(text: &str) -> bool {
    text.parse::<toml::Table>()
        .ok()
        .and_then(|v| v.get("key-mapping").and_then(|k| k.get("preset")).and_then(|p| p.as_str()).map(|p| p == "qwerty"))
        .unwrap_or(true)
}

// ------------------------------------------------------------------ App Shortcuts through `defaults`

fn changed_menu_keys(before: Option<&BTreeMap<String, String>>, title: &str, equiv: String) -> Result<BTreeMap<String, String>, String> {
    if title.contains('\u{1b}') || before.is_some_and(|d| d.keys().any(|k| k.contains('\u{1b}'))) {
        return Err("These App Shortcuts use a stored menu path. Change them in System Settings to preserve that path.".into());
    }
    let mut after = before.cloned().unwrap_or_default();
    after.insert(title.to_string(), equiv);
    Ok(after)
}

/// `defaults` and its domain argument (NSGlobalDomain is `-g`).
fn defaults(args: &[&str]) -> Result<std::process::Output, String> {
    Command::new("/usr/bin/defaults").args(args).output().map_err(|e| format!("cannot run defaults: {e}"))
}

/// The App Shortcuts of one domain now. None when the domain has none.
pub fn read_menu_keys(domain: &str) -> Result<Option<BTreeMap<String, String>>, String> {
    let out = defaults(&["export", domain, "-"])?;
    if !out.status.success() {
        let message = String::from_utf8_lossy(&out.stderr);
        if message.contains("does not exist") || message.contains("not found") {
            return Ok(None);
        }
        return Err(format!("Cannot read the settings of {domain}: {}", message.trim()));
    }
    let v = plist::Value::from_reader(std::io::Cursor::new(out.stdout)).map_err(|e| format!("cannot read the settings of {domain}: {e}"))?;
    let root = v.as_dictionary().ok_or("The preferences root is not a dictionary.")?;
    let Some(value) = root.get("NSUserKeyEquivalents") else { return Ok(None) };
    let d = value.as_dictionary().ok_or("App Shortcuts are not a dictionary. Keyclash did not change them.")?;
    d.iter().map(|(k, v)| v.as_string().map(|s| (k.clone(), s.to_string())).ok_or_else(|| "App Shortcuts contain a value that is not text. Keyclash did not change them.".to_string())).collect::<Result<BTreeMap<_, _>, _>>().map(Some)
}

/// Old-style plist text for a dictionary of strings, for `defaults write`.
fn plist_dict(d: &BTreeMap<String, String>) -> String {
    let q = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let body: Vec<String> = d.iter().map(|(k, v)| format!("{} = {};", q(k), q(v))).collect();
    format!("{{ {} }}", body.join(" "))
}

fn write_menu_keys(domain: &str, d: Option<&BTreeMap<String, String>>) -> Result<(), String> {
    let out = match d {
        Some(d) if !d.is_empty() => defaults(&["write", domain, "NSUserKeyEquivalents", &plist_dict(d)])?,
        _ => {
            let o = defaults(&["delete", domain, "NSUserKeyEquivalents"])?;
            // Deleting a key that is not there is not an error here.
            if !o.status.success() && read_menu_keys(domain)?.is_none() {
                return Ok(());
            }
            o
        }
    };
    if !out.status.success() {
        return Err(format!("defaults could not write {domain}: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let now = read_menu_keys(domain)?;
    let want = d.filter(|d| !d.is_empty()).cloned();
    if now != want {
        return Err(format!("macOS did not keep the change in {domain}. A sandboxed app can need Full Disk Access for this. Use System Settings › Keyboard › Keyboard Shortcuts › App Shortcuts instead."));
    }
    Ok(())
}

// ------------------------------------------------------------------ applying and undoing

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Applied {
    pub plan: Plan,
    pub applied_at: u64,
    /// A copy of the file before the change (file changes only).
    pub backup: Option<String>,
    #[serde(default)]
    pub pending: bool,
}

struct Storage {
    dir: PathBuf,
}

impl Storage {
    fn current() -> Self {
        Self { dir: crate::sources::data_dir().join("agent") }
    }

    fn lock(&self) -> Result<std::fs::File, String> {
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&self.dir)
            .map_err(|e| format!("Cannot create the agent folder: {e}"))?;
        let lock = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false)
            .mode(0o600).open(self.dir.join("agent.lock")).map_err(|e| e.to_string())?;
        lock.try_lock().map_err(|e| format!("Another Keyclash agent operation holds the lock. Retry after it finishes: {e}"))?;
        Ok(lock)
    }

    fn load<T: for<'de> Deserialize<'de>>(&self, name: &str) -> Result<Vec<T>, String> {
        let path = self.dir.join(name);
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("Cannot read {}: {e}. Preserve this file for recovery.", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(format!("Cannot read {}: {e}", path.display())),
        }
    }

    fn store<T: Serialize>(&self, name: &str, list: &[T]) -> Result<(), String> {
        let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
        write_file(&self.dir.join(name), &text)
    }

    fn save_plan(&self, plan: &Plan) -> Result<(), String> {
        let _lock = self.lock()?;
        let mut list: Vec<Plan> = self.load("plans.json")?;
        list.retain(|p| p.plan_id != plan.plan_id);
        list.push(plan.clone());
        let n = list.len();
        if n > 30 { list.drain(..n - 30); }
        self.store("plans.json", &list)
    }

    fn apply(&self, plan: &Plan) -> Result<Applied, String> {
        let _lock = self.lock()?;
        let mut log: Vec<Applied> = self.load("applied.json")?;
        if log.last().is_some_and(|a| a.pending) {
            return Err("A previous write needs recovery. Call undo_last before applying another plan.".into());
        }
        let backup = match &plan.change {
            Change::File { path, before, .. } => {
                if read_text(Path::new(path))? != *before {
                    return Err(format!("{path} changed after the proposal. Propose the change again."));
                }
                let dir = self.dir.join("backups");
                std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir).map_err(|e| e.to_string())?;
                let p = dir.join(format!("{}-{}.txt", unique_stamp(), plan.plan_id));
                write_file(&p, before)?;
                Some(p.display().to_string())
            }
            Change::MenuKeys { domain, before, .. } => {
                if read_menu_keys(domain)? != *before {
                    return Err(format!("The App Shortcuts of {domain} changed after the proposal. Propose the change again."));
                }
                None
            }
        };
        // Persist recovery data before modifying the target. A crash can leave either version.
        let mut done = Applied { plan: plan.clone(), applied_at: crate::sources::now_secs(), backup, pending: true };
        log.push(done.clone());
        self.store("applied.json", &log)?;
        let written = (|| -> Result<(), String> {
            match &plan.change {
                Change::File { path, before, after } => {
                    if read_text(Path::new(path))? != *before {
                        return Err("The file changed during apply.".into());
                    }
                    write_file(Path::new(path), after)?;
                    if read_text(Path::new(path))? != *after {
                        return Err("The file differs from the planned result. Preserve the backup and inspect the file.".into());
                    }
                }
                Change::MenuKeys { domain, after, .. } => write_menu_keys(domain, Some(after))?,
            }
            Ok(())
        })();
        if let Err(e) = written {
            // Nothing reached the target: the recovery record has no use, and it would block later writes.
            if unchanged(&plan.change) {
                log.pop();
                return match self.store("applied.json", &log) {
                    Ok(()) => Err(format!("{e} Keyclash wrote nothing.")),
                    Err(s) => Err(format!("{e} Keyclash wrote nothing, but could not clear the recovery record ({s}). Call undo_last to clear it.")),
                };
            }
            return Err(format!("{e} Call undo_last to recover."));
        }
        done.pending = false;
        *log.last_mut().expect("recovery record") = done.clone();
        self.store("applied.json", &log).map_err(|e| format!("The change was written, but history finalization failed. Call undo_last to recover. {e}"))?;
        Ok(done)
    }

    /// Puts the last change back. The flag is false when Keyclash restored nothing: an apply that
    /// did not finish (a pending record) and left no change of its own, which is only cleared.
    fn undo_last(&self) -> Result<(Applied, bool), String> {
        let _lock = self.lock()?;
        let mut log: Vec<Applied> = self.load("applied.json")?;
        let last = log.pop().ok_or("Keyclash has no change to undo.")?;
        if last.pending && !written_after(&last.plan.change)? {
            // The write never landed, or the person edited the target since. Keyclash has nothing of its own to restore.
            self.store("applied.json", &log)?;
            return Ok((last, false));
        }
        match &last.plan.change {
            Change::File { path, before, after } => {
                let now = read_text(Path::new(path))?;
                if now == *after {
                    write_file(Path::new(path), before)?;
                } else if now != *before {
                    return Err(format!("{path} changed after Keyclash wrote it, so Keyclash did not undo. The backup is at {}.", last.backup.as_deref().unwrap_or("(no backup)")));
                }
            }
            Change::MenuKeys { domain, before, after, .. } => {
                let now = read_menu_keys(domain)?;
                if now.as_ref() == Some(after) {
                    write_menu_keys(domain, before.as_ref())?;
                } else if now != *before {
                    return Err(format!("The App Shortcuts of {domain} changed after Keyclash wrote them, so Keyclash did not undo."));
                }
            }
        }
        self.store("applied.json", &log)?;
        Ok((last, true))
    }
}

/// True when the target still holds the text from before the change.
fn unchanged(change: &Change) -> bool {
    match change {
        Change::File { path, before, .. } => read_text(Path::new(path)).is_ok_and(|now| now == *before),
        Change::MenuKeys { domain, before, .. } => read_menu_keys(domain).is_ok_and(|now| now == *before),
    }
}

/// True when the target holds the planned result.
fn written_after(change: &Change) -> Result<bool, String> {
    Ok(match change {
        Change::File { path, after, .. } => read_text(Path::new(path))? == *after,
        Change::MenuKeys { domain, after, .. } => read_menu_keys(domain)?.as_ref() == Some(after),
    })
}

pub fn save_plan(plan: &Plan) -> Result<(), String> { Storage::current().save_plan(plan) }

pub fn find_plan(plan_id: &str) -> Result<Option<Plan>, String> {
    Ok(Storage::current().load::<Plan>("plans.json")?.into_iter().find(|p| p.plan_id == plan_id))
}

pub fn applied_log() -> Result<Vec<Applied>, String> { Storage::current().load("applied.json") }

fn unique_stamp() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()
}

/// Replace the symlink target atomically. New snapshots are private; configs keep their permissions.
fn write_file(path: &Path, text: &str) -> Result<(), String> {
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let tmp = target.with_file_name(format!(".{name}.keyclash-{}-{}", std::process::id(), unique_stamp()));
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        if let Ok(meta) = std::fs::metadata(&target) { file.set_permissions(meta.permissions())?; }
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, &target)?;
        if let Some(parent) = target.parent() { std::fs::File::open(parent)?.sync_all()?; }
        Ok(())
    })();
    if result.is_err() { let _ = std::fs::remove_file(&tmp); }
    result.map_err(|e| format!("Cannot write {}: {e}", target.display()))
}

pub fn apply(plan: &Plan) -> Result<Applied, String> { Storage::current().apply(plan) }

pub fn undo_last() -> Result<(Applied, bool), String> { Storage::current().undo_last() }

#[cfg(test)]
mod tests {
    use super::*;

    const AERO: &str = "# my config\nstart-at-login = true\n\n[mode.main.binding]\nalt-h = 'focus left'   # left\nalt-shift-h = 'move left'\nctrl-alt-cmd-f = 'fullscreen'\n\n[mode.service.binding]\nalt-h = 'join-with left'\n";

    #[test]
    fn aerospace_edit_changes_one_key_and_keeps_comments() {
        let after = edit_aerospace(AERO, "main", "alt-h", "cmd-alt-j").unwrap();
        assert!(after.contains("cmd-alt-j = 'focus left'   # left\n"));
        assert!(after.contains("[mode.service.binding]\nalt-h = 'join-with left'"));
        assert_eq!(after.lines().count(), AERO.lines().count());
        check_aerospace(AERO, &after, "main", "alt-h", "cmd-alt-j").unwrap();
    }

    #[test]
    fn aerospace_refuses_a_key_that_is_already_bound() {
        assert!(edit_aerospace(AERO, "main", "alt-h", "alt-shift-h").unwrap_err().contains("already has"));
        assert!(edit_aerospace(AERO, "main", "alt-x", "alt-y").is_err());
    }

    #[test]
    fn aerospace_check_catches_extra_changes() {
        let bad = AERO.replace("'fullscreen'", "'layout floating'").replace("alt-h = 'focus left'", "cmd-alt-j = 'focus left'");
        assert!(check_aerospace(AERO, &bad, "main", "alt-h", "cmd-alt-j").is_err());
    }

    #[test]
    fn spells_keys_for_each_tool() {
        let t = Trigger::combo(CTRL | OPT | CMD, "Left");
        assert_eq!(aerospace_binding(&t, false).unwrap(), "cmd-alt-ctrl-left");
        assert_eq!(omniwm_binding(&t).unwrap(), "Control+Option+Command+Left Arrow");
        assert_eq!(skhd_binding(&t).unwrap(), "cmd + alt + ctrl - left");
        assert_eq!(parse_plus("Control+Option+Command+Left Arrow"), Some(t.clone()));
        assert_eq!(skhd_trigger("cmd + alt + ctrl - left"), Some(t));
        let k = Trigger::combo(CMD | SHIFT, "K");
        assert_eq!(menu_equivalent(&k).unwrap(), "$@k");
        assert_eq!(parse_user_key_equivalent("$@k"), Some(k));
        assert_eq!(aerospace_binding(&Trigger::combo(OPT, ";"), false).unwrap(), "alt-semicolon");
        assert_eq!(parse_dash("alt-semicolon"), Some(Trigger::combo(OPT, ";")));
        assert!(skhd_binding(&Trigger::combo(OPT, ";")).is_err());
        assert!(menu_equivalent(&Trigger::combo(SHIFT, "K")).is_err());
        assert!(aerospace_binding(&Trigger::ModTap { mods: OPT, taps: 2 }, false).is_err());
        assert!(aerospace_binding(&Trigger::combo(CMD, "F99"), false).is_err());
        assert!(skhd_binding(&Trigger::combo(CMD, "Forward Delete")).is_err());
        assert_eq!(parse_dash("alt--"), Some(Trigger::combo(OPT, "-")));
    }

    #[test]
    fn menu_equivalent_reads_back_for_arrows_and_function_keys() {
        for t in [Trigger::combo(CMD | OPT, "Left"), Trigger::combo(CTRL, "F5"), Trigger::combo(CMD, "Page Down")] {
            let e = menu_equivalent(&t).unwrap();
            assert_eq!(parse_user_key_equivalent(&e), Some(t));
        }
    }

    #[test]
    fn omniwm_edit_touches_only_that_hotkey() {
        let text = "[[hotkeys]]\nid = \"focusLeft\"\nbinding = \"Option+H\"\n\n[[hotkeys]]\nid = \"focusRight\"\nbinding = \"Option+H\"\n";
        let after = edit_omniwm(text, "focusRight", "Option+H", "Option+L").unwrap();
        assert_eq!(after, text.replacen("binding = \"Option+H\"\n\n[[hotkeys]]\nid = \"focusRight\"\nbinding = \"Option+H\"", "binding = \"Option+H\"\n\n[[hotkeys]]\nid = \"focusRight\"\nbinding = \"Option+L\"", 1));
        check_omniwm(text, &after, "focusRight", "Option+L").unwrap();
        assert!(check_omniwm(text, &after, "focusLeft", "Option+L").is_err());

        let inline = "hotkeys = [ { id = \"a\", binding = \"Option+H\" }, { id = \"b\", binding = \"Option+H\" } ]\n";
        assert!(edit_omniwm(inline, "a", "Option+H", "Option+J").is_err());
    }

    #[test]
    fn skhd_edit_keeps_mode_and_command() {
        let text = "# skhd\nalt - h : yabai -m window --focus west\nresize < alt - h : yabai -m window --resize left:-20:0\n";
        let old = Trigger::combo(OPT, "H");
        let after = edit_skhd(text, &old, "yabai -m window --focus west", "cmd + alt - j").unwrap();
        assert_eq!(after, "# skhd\ncmd + alt - j : yabai -m window --focus west\nresize < alt - h : yabai -m window --resize left:-20:0\n");
        let after = edit_skhd(text, &old, "yabai -m window --resize left:-20:0", "ctrl - h").unwrap();
        assert!(after.contains("resize < ctrl - h : yabai"));
        assert!(edit_skhd(text, &old, "nothing", "ctrl - h").is_err());
    }

    #[test]
    fn writers_by_source() {
        let s = Shortcut::new("AeroSpace", "focus left", Trigger::combo(OPT, "H"), Scope::Global, Origin::ConfigFile)
            .bundle(Some(AEROSPACE))
            .detail("/Users/me/.aerospace.toml · [mode.main.binding] alt-h");
        assert_eq!(writer_for(&s).unwrap(), Writer::Aerospace { path: "/Users/me/.aerospace.toml".into(), mode: "main".into(), key: "alt-h".into() });
        let s = Shortcut::new("OmniWM", "Focus Left", Trigger::combo(OPT, "H"), Scope::Global, Origin::ConfigFile)
            .bundle(Some(OMNIWM))
            .detail("/Users/me/.config/omniwm/settings.toml · focusLeft = \"Option+H\"");
        assert_eq!(writer_for(&s).unwrap(), Writer::Omniwm { path: "/Users/me/.config/omniwm/settings.toml".into(), id: "focusLeft".into() });
        let s = Shortcut::new("Mail", "Archive", Trigger::combo(CMD | CTRL, "A"), Scope::App, Origin::MenuFile).bundle(Some("com.apple.mail"));
        assert_eq!(writer_for(&s).unwrap(), Writer::MenuKeys { domain: "com.apple.mail".into(), title: "Archive".into() });
        let s = Shortcut::new("macOS", "Mission Control", Trigger::combo(CTRL, "Up"), Scope::Global, Origin::SystemSettings).bundle(Some(crate::sources::MACOS));
        assert!(writer_for(&s).unwrap_err().contains("System Settings"));
    }

    #[test]
    fn plist_text_escapes_quotes() {
        let d: BTreeMap<String, String> = [("Say \"hi\"".to_string(), "@k".to_string())].into_iter().collect();
        assert_eq!(plist_dict(&d), r#"{ "Say \"hi\"" = "@k"; }"#);
    }

    #[test]
    fn a_leaf_title_cannot_bypass_an_existing_menu_path() {
        let before = BTreeMap::from([("\u{1b}File\u{1b}Open".into(), "@o".into())]);
        assert!(changed_menu_keys(Some(&before), "Open", "@j".into()).unwrap_err().contains("stored menu path"));
        assert!(changed_menu_keys(None, "File\u{1b}Open", "@j".into()).is_err());
        let before = BTreeMap::from([("Save".into(), "@s".into())]);
        let after = changed_menu_keys(Some(&before), "Open", "@j".into()).unwrap();
        assert_eq!(after["Save"], "@s");
        assert_eq!(after["Open"], "@j");
    }

    #[test]
    fn apply_and_undo_a_file_change() {
        let dir = std::env::temp_dir().join(format!("keyclash-remap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = Storage { dir: dir.join("data") };
        let cfg = dir.join("aerospace.toml");
        std::fs::write(&cfg, AERO).unwrap();
        let s = Shortcut::new("AeroSpace", "focus left", Trigger::combo(OPT, "H"), Scope::Global, Origin::ConfigFile)
            .bundle(Some(AEROSPACE))
            .detail(format!("{} · [mode.main.binding] alt-h", cfg.display()));
        let plan = propose(&s, "k1", &Trigger::combo(OPT | CMD, "J")).unwrap();
        assert!(plan.diff().contains("- alt-h = 'focus left'"));
        assert!(plan.diff().contains("+ cmd-alt-j = 'focus left'"));
        storage.save_plan(&plan).unwrap();
        assert_eq!(storage.load::<Plan>("plans.json").unwrap()[0].plan_id, plan.plan_id);
        let done = storage.apply(&plan).unwrap();
        assert!(std::fs::read_to_string(&cfg).unwrap().contains("cmd-alt-j = 'focus left'"));
        assert_eq!(std::fs::read_to_string(done.backup.unwrap()).unwrap(), AERO);
        // A second apply of the same plan is stale: the file changed.
        assert!(storage.apply(&plan).unwrap_err().contains("changed after"));
        storage.undo_last().unwrap();
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), AERO);
        assert!(storage.undo_last().is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn fixture() -> (Storage, PathBuf, Plan) {
        let dir = std::env::temp_dir().join(format!("keyclash-remap-{}-{}", std::process::id(), unique_stamp()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join("aerospace.toml");
        std::fs::write(&cfg, AERO).unwrap();
        let s = Shortcut::new("AeroSpace", "focus left", Trigger::combo(OPT, "H"), Scope::Global, Origin::ConfigFile)
            .bundle(Some(AEROSPACE)).detail(format!("{} · [mode.main.binding] alt-h", cfg.display()));
        let plan = propose(&s, "k1", &Trigger::combo(OPT | CMD, "J")).unwrap();
        (Storage { dir: dir.join("data") }, cfg, plan)
    }

    #[test]
    fn stale_apply_and_undo_preserve_external_edits() {
        let (storage, cfg, plan) = fixture();
        std::fs::write(&cfg, format!("{AERO}# external edit\n")).unwrap();
        assert!(storage.apply(&plan).unwrap_err().contains("changed after"));
        assert!(!storage.dir.join("applied.json").exists());
        std::fs::write(&cfg, AERO).unwrap();
        storage.apply(&plan).unwrap();
        let external = format!("{}# another edit\n", read_text(&cfg).unwrap());
        std::fs::write(&cfg, &external).unwrap();
        assert!(storage.undo_last().unwrap_err().contains("did not undo"));
        assert_eq!(read_text(&cfg).unwrap(), external);
        assert_eq!(storage.load::<Applied>("applied.json").unwrap().len(), 1);
    }

    #[test]
    fn recovery_record_handles_crashes_before_or_after_target_write() {
        let (storage, cfg, plan) = fixture();
        let lock = storage.lock().unwrap();
        let pending = Applied { plan: plan.clone(), applied_at: 0, backup: None, pending: true };
        storage.store("applied.json", std::slice::from_ref(&pending)).unwrap();
        drop(lock);
        assert!(storage.apply(&plan).unwrap_err().contains("needs recovery"));
        // The crash came before the write: undo clears the record and restores nothing.
        assert!(!storage.undo_last().unwrap().1);
        assert_eq!(read_text(&cfg).unwrap(), AERO);
        assert!(storage.load::<Applied>("applied.json").unwrap().is_empty());
        let lock = storage.lock().unwrap();
        storage.store("applied.json", &[pending]).unwrap();
        if let Change::File { after, .. } = &plan.change { write_file(&cfg, after).unwrap(); }
        drop(lock);
        assert!(storage.undo_last().unwrap().1);
        assert_eq!(read_text(&cfg).unwrap(), AERO);
        // A pending record whose target the person edited by hand is cleared, not a dead end.
        let lock = storage.lock().unwrap();
        storage.store("applied.json", &[Applied { plan: plan.clone(), applied_at: 0, backup: None, pending: true }]).unwrap();
        drop(lock);
        std::fs::write(&cfg, format!("{AERO}# hand edit\n")).unwrap();
        assert!(!storage.undo_last().unwrap().1);
        assert_eq!(read_text(&cfg).unwrap(), format!("{AERO}# hand edit\n"));
        std::fs::write(&cfg, AERO).unwrap();
        storage.apply(&plan).unwrap();
    }

    #[test]
    fn locked_or_corrupt_history_prevents_writes() {
        let (storage, cfg, plan) = fixture();
        let lock = storage.lock().unwrap();
        assert!(storage.apply(&plan).unwrap_err().contains("holds the lock"));
        drop(lock);
        std::fs::write(storage.dir.join("applied.json"), "broken").unwrap();
        assert!(storage.apply(&plan).unwrap_err().contains("Preserve this file"));
        assert_eq!(read_text(&cfg).unwrap(), AERO);
    }

    #[test]
    fn symlinks_and_permissions_survive_apply_and_undo() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let (storage, cfg, mut plan) = fixture();
        let link = cfg.with_extension("link");
        symlink(&cfg, &link).unwrap();
        std::fs::set_permissions(&cfg, std::fs::Permissions::from_mode(0o640)).unwrap();
        if let Change::File { path, .. } = &mut plan.change { *path = link.display().to_string(); }
        let done = storage.apply(&plan).unwrap();
        assert!(link.is_symlink());
        assert_eq!(std::fs::metadata(&cfg).unwrap().permissions().mode() & 0o777, 0o640);
        assert_eq!(std::fs::metadata(done.backup.unwrap()).unwrap().permissions().mode() & 0o777, 0o600);
        storage.undo_last().unwrap();
        assert!(link.is_symlink());
        assert_eq!(read_text(&cfg).unwrap(), AERO);
    }

    #[test]
    fn skhd_refuses_duplicate_keys_and_multiline_commands() {
        let old = Trigger::combo(OPT, "H");
        assert!(edit_skhd("alt - h : left\nalt - j : down\n", &old, "left", "alt - j").unwrap_err().contains("already uses"));
        assert!(edit_skhd("alt - h : left \\\n next\n", &old, "left", "alt - j").unwrap_err().contains("continuations"));
    }
}
