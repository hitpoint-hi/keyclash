//! Electron apps build their menus in JavaScript inside `app.asar`.
//! This scans that file for `accelerator: "CmdOrCtrl+Shift+T"` (menu items)
//! and `globalShortcut.register("…")` (system-wide hotkeys).

use crate::keys::{clean_trigger, modifier_word, normalize_key};
use crate::model::*;
use memchr::memmem;
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

const CHUNK: usize = 16 * 1024 * 1024;
const OVERLAP: usize = 1024;

/// Electron accelerator text to a trigger.
pub fn parse_accelerator(acc: &str) -> Option<Trigger> {
    let parts: Vec<&str> = acc.split('+').collect();
    if parts.is_empty() {
        return None;
    }
    let mut mods = 0u8;
    let mut key = None;
    let n = parts.len();
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() {
            // "CmdOrCtrl++" means the plus key.
            if i == n - 1 {
                key = Some("+".to_string());
            }
            continue;
        }
        if i < n - 1 {
            mods |= modifier_word(p)?;
        } else if let Some(m) = modifier_word(p) {
            // Only modifiers: not a usable accelerator.
            let _ = m;
            return None;
        } else {
            // A backslash is written escaped in the app's code.
            key = Some(normalize_key(if *p == "\\\\" { "\\" } else { p }));
        }
    }
    let key = key?;
    if key.is_empty() || mods == 0 && key.chars().count() == 1 {
        // A bare letter is not a shortcut in a menu.
        return None;
    }
    Some(clean_trigger(mods, key))
}

fn looks_like_accelerator(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 40
        && s.chars().all(|c| c.is_ascii_alphanumeric() || "+-=[],./;'`\\ ".contains(c))
        && (s.contains('+') || s.starts_with('F'))
}

/// The first quoted string within `limit` bytes after `p`.
fn quoted_after(buf: &[u8], mut p: usize, limit: usize, max_len: usize) -> Option<String> {
    let end = (p + limit).min(buf.len());
    while p < end {
        let c = buf[p];
        if c == b'"' || c == b'\'' || c == b'`' {
            let q = c;
            let start = p + 1;
            let mut e = start;
            while e < buf.len() && buf[e] != q && e - start <= max_len {
                if buf[e] == b'\n' {
                    return None;
                }
                e += 1;
            }
            if e < buf.len() && buf[e] == q {
                return std::str::from_utf8(&buf[start..e]).ok().map(String::from);
            }
            return None;
        }
        p += 1;
    }
    None
}

/// The nearest `label` before an accelerator, inside the same object literal.
fn label_before(buf: &[u8], pos: usize) -> Option<String> {
    let start = pos.saturating_sub(400);
    let window = &buf[start..pos];
    let mut best = None;
    for i in memmem::find_iter(window, b"label") {
        best = Some(i);
    }
    let i = best?;
    // Stop if another accelerator sits between label and this one.
    if memmem::find(&window[i..], b"accelerator").is_some() {
        return None;
    }
    let label = quoted_after(buf, start + i + 5, 40, 80)?;
    let clean = label.replace('&', "");
    let trimmed = clean.trim();
    if trimmed.is_empty() || trimmed.len() > 80 {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Turn a raw label into words, or None when it is an id, a code or a translation key.
fn clean_label(raw: &str) -> Option<String> {
    let l = raw.replace("\\u2026", "…").replace("\\u2019", "’");
    if looks_like_accelerator(&l) && parse_accelerator(&l).is_some() {
        return None;
    }
    if !l.contains(' ') {
        // "browserSidebar.zoomBanner.zoomOut" -> "Zoom Out"; "H3oH0g" -> None.
        let last = l.rsplit('.').next().unwrap_or(&l);
        let has_digit = last.chars().any(|c| c.is_ascii_digit());
        let upper_inside = last.chars().skip(1).filter(|c| c.is_uppercase()).count();
        if has_digit || last.len() < 3 || (upper_inside >= 2 && last.len() <= 8 && !l.contains('.')) {
            return None;
        }
        if l.contains('.') || upper_inside > 0 {
            return Some(words(last));
        }
    }
    Some(l)
}

fn words(id: &str) -> String {
    let mut out = String::new();
    for (i, c) in id.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            out.push(' ');
        }
        if i == 0 {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
    }
    out.replace(['_', '-'], " ")
}

/// Menu items that only give a role ("undo", "togglefullscreen").
fn role_near(buf: &[u8], pos: usize) -> Option<String> {
    // Stay inside this item's own object literal: from the last "{" to the next "}".
    let lo = pos.saturating_sub(300);
    let start = lo + memchr::memrchr(b'{', &buf[lo..pos])?;
    let hi = (pos + 300).min(buf.len());
    let end = pos + memchr::memchr(b'}', &buf[pos..hi])?;
    let i = memmem::find(&buf[start..end], b"role")?;
    let role = quoted_after(buf, start + i + 4, 6, 40)?;
    if role.is_empty() || !role.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some(match role.to_lowercase().as_str() {
        "togglefullscreen" => "Toggle Full Screen".into(),
        "pasteandmatchstyle" => "Paste and Match Style".into(),
        "selectall" => "Select All".into(),
        "zoomin" => "Zoom In".into(),
        "zoomout" => "Zoom Out".into(),
        "resetzoom" => "Actual Size".into(),
        "forcereload" => "Force Reload".into(),
        "toggledevtools" => "Toggle Developer Tools".into(),
        "hideothers" => "Hide Others".into(),
        _ => words(&role),
    })
}

pub struct ElectronFind {
    pub menu: Vec<(String, Trigger)>,
    pub global: Vec<(String, Trigger)>,
}

pub fn scan_asar(path: &Path) -> std::io::Result<ElectronFind> {
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; CHUNK + OVERLAP];
    let mut carry = 0usize;
    let mut seen = HashSet::new();
    let mut out = ElectronFind { menu: Vec::new(), global: Vec::new() };
    let acc_finder = memmem::Finder::new(b"accelerator");
    let gs_finder = memmem::Finder::new(b"globalShortcut.register(");
    loop {
        let n = f.read(&mut buf[carry..])?;
        if n == 0 {
            break;
        }
        let len = carry + n;
        let data = &buf[..len];
        for pos in acc_finder.find_iter(data) {
            let after = pos + "accelerator".len();
            // Only `accelerator:` / `"accelerator":` / `accelerator =` forms.
            let mut q = after;
            while q < len && (data[q] == b'"' || data[q] == b' ') {
                q += 1;
            }
            if q >= len || (data[q] != b':' && data[q] != b'=') {
                continue;
            }
            let Some(acc) = quoted_after(data, q + 1, 4, 40) else { continue };
            if !looks_like_accelerator(&acc) {
                continue;
            }
            let Some(t) = parse_accelerator(&acc) else { continue };
            let label = label_before(data, pos)
                .and_then(|l| clean_label(&l))
                .or_else(|| role_near(data, pos))
                .unwrap_or_else(|| "(menu item)".into());
            if seen.insert((label.clone(), t.clone(), false)) {
                out.menu.push((label, t));
            }
        }
        for pos in gs_finder.find_iter(data) {
            let p = pos + "globalShortcut.register(".len();
            let Some(acc) = quoted_after(data, p, 2, 40) else { continue };
            let Some(t) = parse_accelerator(&acc) else { continue };
            // A bare key registered globally is only held while a popup is open.
            if t.mods() == 0 {
                continue;
            }
            let label = "Global hotkey".to_string();
            if seen.insert((label.clone(), t.clone(), true)) {
                out.global.push((label, t));
            }
        }
        // Keep the tail so a match split across chunks is still found.
        let keep = OVERLAP.min(len);
        buf.copy_within(len - keep..len, 0);
        carry = keep;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accelerators() {
        assert_eq!(parse_accelerator("CmdOrCtrl+Shift+T"), Some(Trigger::combo(CMD | SHIFT, "T")));
        assert_eq!(parse_accelerator("Command+Control+F"), Some(Trigger::combo(CMD | CTRL, "F")));
        // "Plus" needs Shift: the same keys as ⇧⌘=.
        assert_eq!(parse_accelerator("CmdOrCtrl+Plus"), Some(Trigger::combo(CMD | SHIFT, "=")));
        assert_eq!(parse_accelerator("CmdOrCtrl+\\\\"), Some(Trigger::combo(CMD, "\\")));
        assert_eq!(parse_accelerator("CmdOrCtrl+-"), Some(Trigger::combo(CMD, "-")));
    }
}
