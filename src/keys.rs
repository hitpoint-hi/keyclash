//! Key names, key codes, modifier flag formats and how a trigger is written.

use crate::model::*;

/// US-ANSI virtual key codes (Carbon `kVK_*`).
pub fn keycode_name(code: i64) -> Option<&'static str> {
    Some(match code {
        0 => "A", 1 => "S", 2 => "D", 3 => "F", 4 => "H", 5 => "G", 6 => "Z", 7 => "X",
        8 => "C", 9 => "V", 10 => "§", 11 => "B", 12 => "Q", 13 => "W", 14 => "E", 15 => "R",
        16 => "Y", 17 => "T", 18 => "1", 19 => "2", 20 => "3", 21 => "4", 22 => "6", 23 => "5",
        24 => "=", 25 => "9", 26 => "7", 27 => "-", 28 => "8", 29 => "0", 30 => "]", 31 => "O",
        32 => "U", 33 => "[", 34 => "I", 35 => "P", 36 => "Return", 37 => "L", 38 => "J",
        39 => "'", 40 => "K", 41 => ";", 42 => "\\", 43 => ",", 44 => "/", 45 => "N", 46 => "M",
        47 => ".", 48 => "Tab", 49 => "Space", 50 => "`", 51 => "Delete", 52 => "Enter",
        53 => "Escape", 57 => "Caps Lock", 64 => "F17", 65 => "Keypad .", 67 => "Keypad *",
        69 => "Keypad +", 71 => "Clear", 75 => "Keypad /", 76 => "Keypad Enter", 78 => "Keypad -",
        79 => "F18", 80 => "F19", 81 => "Keypad =", 82 => "Keypad 0", 83 => "Keypad 1",
        84 => "Keypad 2", 85 => "Keypad 3", 86 => "Keypad 4", 87 => "Keypad 5", 88 => "Keypad 6",
        89 => "Keypad 7", 90 => "F20", 91 => "Keypad 8", 92 => "Keypad 9", 96 => "F5", 97 => "F6",
        98 => "F7", 99 => "F3", 100 => "F8", 101 => "F9", 103 => "F11", 105 => "F13",
        106 => "F16", 107 => "F14", 109 => "F10", 111 => "F12", 113 => "F15", 114 => "Help",
        115 => "Home", 116 => "Page Up", 117 => "Forward Delete", 118 => "F4", 119 => "End",
        120 => "F2", 121 => "Page Down", 122 => "F1", 123 => "Left", 124 => "Right",
        125 => "Down", 126 => "Up",
        _ => return None,
    })
}

/// The key name for a key code in the keyboard layout in use. A hotkey that is stored as a
/// key code follows the physical key, so its letter is the one that key types now.
/// `keycode_name` gives the US name, which is right only for the US layout.
pub fn keycode_key(code: i64) -> Option<String> {
    crate::layout::key_at(code).or_else(|| keycode_name(code).map(String::from))
}

/// Key codes of the modifier keys themselves.
pub fn keycode_modifier(code: i64) -> Option<u8> {
    match code {
        54 | 55 => Some(CMD),
        56 | 60 => Some(SHIFT),
        58 | 61 => Some(OPT),
        59 | 62 => Some(CTRL),
        63 => Some(FN),
        _ => None,
    }
}

/// Keys that macOS marks with the function-key flag on their own.
pub fn is_function_key(key: &str) -> bool {
    matches!(
        key,
        "Left" | "Right" | "Up" | "Down" | "Home" | "End" | "Page Up" | "Page Down"
            | "Forward Delete" | "Help" | "Clear"
    ) || (key.starts_with('F') && key.len() > 1 && key[1..].chars().all(|c| c.is_ascii_digit()))
}

/// NSEvent modifier flags (AppKit, symbolichotkeys, Rectangle, nib files).
pub const NS_SHIFT: i64 = 131072;
pub const NS_CONTROL: i64 = 262144;
pub const NS_OPTION: i64 = 524288;
pub const NS_COMMAND: i64 = 1048576;
pub const NS_FN: i64 = 8388608;

/// Modifier bits back to NSEvent flags.
pub fn mods_to_nsevent(m: u8) -> i64 {
    let mut f = 0;
    if m & SHIFT != 0 { f |= NS_SHIFT; }
    if m & CTRL != 0 { f |= NS_CONTROL; }
    if m & OPT != 0 { f |= NS_OPTION; }
    if m & CMD != 0 { f |= NS_COMMAND; }
    if m & FN != 0 { f |= NS_FN; }
    f
}

pub fn mods_from_nsevent(flags: i64) -> u8 {
    let mut m = 0;
    if flags & NS_SHIFT != 0 { m |= SHIFT; }
    if flags & NS_CONTROL != 0 { m |= CTRL; }
    if flags & NS_OPTION != 0 { m |= OPT; }
    if flags & NS_COMMAND != 0 { m |= CMD; }
    if flags & NS_FN != 0 { m |= FN; }
    m
}

/// Carbon modifier flags (cmdKey 256, shiftKey 512, optionKey 2048, controlKey 4096).
pub fn mods_from_carbon(flags: i64) -> u8 {
    let mut m = 0;
    if flags & 256 != 0 { m |= CMD; }
    if flags & 512 != 0 { m |= SHIFT; }
    if flags & 2048 != 0 { m |= OPT; }
    if flags & 4096 != 0 { m |= CTRL; }
    m
}

/// Accessibility menu modifier bits: shift 1, option 2, control 4, 8 = no Command.
/// AXMenuItemCmdModifiers bits to NSEvent flags.
/// 1 -> shift 0x20000, 2 -> option 0x80000, 4 -> control 0x40000,
/// 8 clear -> command 0x100000, 16 with a value of 24 or more -> fn 0x800000.
pub fn ax_menu_to_nsevent(bits: i64) -> i64 {
    let mut f = 0;
    if bits & 1 != 0 { f |= NS_SHIFT; }
    if bits & 2 != 0 { f |= NS_OPTION; }
    if bits & 4 != 0 { f |= NS_CONTROL; }
    if bits & 8 == 0 { f |= NS_COMMAND; }
    if bits & 16 != 0 && bits >= 24 { f |= NS_FN; }
    f
}

pub fn mods_from_ax_menu(bits: i64) -> u8 {
    mods_from_nsevent(ax_menu_to_nsevent(bits))
}

/// A symbol that needs Shift on a US keyboard, as the key it sits on ("?" is on "/").
pub fn unshifted(key: &str) -> Option<&'static str> {
    Some(match key {
        "?" => "/", "+" => "=", "{" => "[", "}" => "]", ":" => ";", "\"" => "'",
        "<" => ",", ">" => ".", "~" => "`", "|" => "\\", "_" => "-",
        "!" => "1", "@" => "2", "#" => "3", "$" => "4", "%" => "5",
        "^" => "6", "&" => "7", "*" => "8", "(" => "9", ")" => "0",
        _ => return None,
    })
}

/// The key that types `key` with Shift in `layout` ("+" gives "=" in US, and nothing in German,
/// where "+" has its own key). None when a key types it without Shift.
/// A layout read without Shift data, or a character the layout cannot type, uses the US table.
pub fn shifted_base_in(key: &str, layout: Option<&crate::layout::Layout>) -> Option<String> {
    let us = || unshifted(key).map(String::from);
    let Some(l) = layout.filter(|l| !l.shifted.is_empty()) else { return us() };
    if l.keys.values().any(|k| k == key) {
        return None;
    }
    let code = l.shifted.iter().filter(|(_, up)| up.as_str() == key).map(|(c, _)| *c).min();
    match code.and_then(|c| l.keys.get(&c)) {
        Some(base) => Some(base.clone()),
        None => us(),
    }
}

/// `shifted_base_in` for the layout in use.
pub fn shifted_base(key: &str) -> Option<String> {
    crate::layout::with_active(|layout| shifted_base_in(key, layout))
}

/// `clean_trigger` with the US Shift table, for keys named by their place on a US keyboard
/// (AeroSpace, Karabiner). `layout::key_on_us_position` then gives the key of the layout in use.
pub fn clean_trigger_us(mods: u8, key: String) -> Trigger {
    let (mods, key) = match unshifted(&key) {
        Some(base) => (mods | SHIFT, base.to_string()),
        None => (mods, key),
    };
    let mods = if is_function_key(&key) { mods & !FN } else { mods };
    Trigger::Combo { mods, key }
}

/// One spelling per physical chord: "⌘?" and "⇧⌘/" are the same keys, so both become ⇧⌘/.
/// The layout in use decides which characters need Shift. Use it for keys named by character.
/// Also removes the function-key flag that arrows and F-keys carry anyway.
pub fn clean_trigger(mods: u8, key: String) -> Trigger {
    let (mods, key) = match shifted_base(&key) {
        Some(base) => (mods | SHIFT, base),
        None => (mods, key),
    };
    let mods = if is_function_key(&key) { mods & !FN } else { mods };
    Trigger::Combo { mods, key }
}

/// A menu key-equivalent character (AppKit / AX), to a key name and an implied Shift.
pub fn key_from_char(s: &str) -> Option<(String, bool)> {
    let mut chars = s.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        // Several characters: keep as written.
        return Some((s.to_uppercase(), false));
    }
    let name = match c as u32 {
        0x0D | 0x0A => "Return",
        0x03 => "Enter",
        0x09 => "Tab",
        0x19 => return Some(("Tab".into(), true)),
        0x1B => "Escape",
        0x08 | 0x7F => "Delete",
        0x20 => "Space",
        0xF700 => "Up",
        0xF701 => "Down",
        0xF702 => "Left",
        0xF703 => "Right",
        0xF704..=0xF71B => return Some((format!("F{}", c as u32 - 0xF704 + 1), false)),
        0xF728 => "Forward Delete",
        0xF729 => "Home",
        0xF72B => "End",
        0xF72C => "Page Up",
        0xF72D => "Page Down",
        0xF746 => "Help",
        0xF739 => "Clear",
        _ => {
            if c.is_alphabetic() && c.is_uppercase() {
                return Some((c.to_string(), true));
            }
            // Control characters, unmapped private-use glyphs and emoji (the 🎤 dictation key) are not keys.
            let u = c as u32;
            if u < 0x20 || (0xE000..=0xF8FF).contains(&u) || u >= 0x1F000 {
                return None;
            }
            return Some((c.to_uppercase().to_string(), false));
        }
    };
    Some((name.to_string(), false))
}

/// The same for `AXMenuItemCmdChar`. Accessibility always gives letters in upper case and
/// reports Shift in `AXMenuItemCmdModifiers` (bit 1), so an upper-case letter does not mean Shift here.
pub fn key_from_ax_char(s: &str) -> Option<(String, bool)> {
    let (key, shift) = key_from_char(s)?;
    let mut chars = s.chars();
    let one_letter = chars.next().is_some_and(|c| c.is_alphabetic()) && chars.next().is_none();
    Some((key, shift && !one_letter))
}

/// Carbon menu glyph codes (kMenu*Glyph) used by AXMenuItemCmdGlyph.
pub fn key_from_glyph(g: i64) -> Option<&'static str> {
    Some(match g {
        0x02 | 0x03 => "Tab",
        0x04 => "Enter",
        0x09 => "Space",
        0x0A => "Forward Delete",
        0x0B => "Return",
        0x17 => "Delete",
        0x1B => "Escape",
        0x1C => "Clear",
        0x62 => "Page Up",
        0x64 => "Left",
        0x65 => "Right",
        0x66 => "Home",
        0x68 => "Up",
        0x69 => "End",
        0x6A => "Down",
        0x6B => "Page Down",
        0x6F => "F1", 0x70 => "F2", 0x71 => "F3", 0x72 => "F4", 0x73 => "F5", 0x74 => "F6",
        0x75 => "F7", 0x76 => "F8", 0x77 => "F9", 0x78 => "F10", 0x79 => "F11", 0x7A => "F12",
        0x87 => "F13", 0x88 => "F14", 0x89 => "F15",
        _ => return None,
    })
}

/// A written key name from a config file or code ("left", "pageUp", "minus") to the canonical name.
pub fn normalize_key(raw: &str) -> String {
    let t = raw.trim();
    if t.chars().count() == 1 {
        return t.to_uppercase();
    }
    let low: String = t.to_lowercase().chars().filter(|c| !c.is_whitespace() && *c != '_').collect();
    let s = match low.as_str() {
        "left" | "leftarrow" | "arrowleft" => "Left",
        "right" | "rightarrow" | "arrowright" => "Right",
        "up" | "uparrow" | "arrowup" => "Up",
        "down" | "downarrow" | "arrowdown" => "Down",
        "enter" | "return" => "Return",
        "keypadenter" | "numpadenter" => "Keypad Enter",
        "space" | "spacebar" => "Space",
        "tab" => "Tab",
        "esc" | "escape" => "Escape",
        "backspace" | "delete" | "deleteorbackspace" => "Delete",
        "forwarddelete" | "deleteforward" | "del" => "Forward Delete",
        "pageup" => "Page Up",
        "pagedown" => "Page Down",
        "home" => "Home",
        "end" => "End",
        "minus" | "hyphen" => "-",
        "equal" | "equals" | "equalsign" => "=",
        "plus" => "+",
        "comma" => ",",
        "period" | "dot" => ".",
        "slash" => "/",
        "backslash" => "\\",
        "semicolon" => ";",
        "quote" | "apostrophe" | "singlequote" => "'",
        "backtick" | "grave" | "graveaccentandtilde" => "`",
        "leftsquarebracket" | "bracketleft" | "openbracket" | "leftbracket" => "[",
        "rightsquarebracket" | "bracketright" | "closebracket" | "rightbracket" => "]",
        "questionmark" => "?",
        "colon" => ":",
        "pipe" => "|",
        "exclamationmark" => "!",
        "opencurlybracket" => "{",
        "closecurlybracket" => "}",
        "capslock" => "Caps Lock",
        "numadd" => "Keypad +",
        "numsub" => "Keypad -",
        "nummult" => "Keypad *",
        "numdiv" => "Keypad /",
        "numdec" => "Keypad .",
        "help" | "insert" => "Help",
        _ => {
            if let Some(n) = low.strip_prefix('f') {
                if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) {
                    return format!("F{n}");
                }
            }
            if let Some(n) = low.strip_prefix("num").filter(|n| n.len() == 1 && n.chars().all(|c| c.is_ascii_digit())) {
                return format!("Keypad {n}");
            }
            if let Some(n) = low.strip_prefix("keypad") {
                return format!("Keypad {}", n.to_uppercase());
            }
            let mut c = t.chars();
            return match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            };
        }
    };
    s.to_string()
}

/// A modifier word in a config file, to modifier bits.
/// skhd modifier words and their NSEvent flags.
pub const SKHD_MODIFIERS: [(&str, i64); 15] = [
    ("shift", 0x20000), ("lshift", 0x20000), ("rshift", 0x20000),
    ("ctrl", 0x40000), ("lctrl", 0x40000), ("rctrl", 0x40000),
    ("cmd", 0x100000), ("lcmd", 0x100000), ("rcmd", 0x100000),
    ("alt", 0x80000), ("lalt", 0x80000), ("ralt", 0x80000),
    ("fn", 8388608), ("hyper", 1966080), ("meh", 917504),
];

pub fn modifier_word(w: &str) -> Option<u8> {
    let raw = w.trim().to_lowercase();
    if let Some((_, f)) = SKHD_MODIFIERS.iter().find(|(n, _)| *n == raw) {
        return Some(mods_from_nsevent(*f));
    }
    let l = raw;
    let l = l.trim_start_matches("left_").trim_start_matches("right_").trim_start_matches("left").trim_start_matches("right");
    match l {
        "cmd" | "command" | "super" | "meta" | "cmdorctrl" | "commandorcontrol" => Some(CMD),
        "shift" => Some(SHIFT),
        "alt" | "opt" | "option" | "altgr" => Some(OPT),
        "ctrl" | "control" => Some(CTRL),
        "fn" | "function" | "globe" => Some(FN),
        "hyper" => Some(CMD | SHIFT | OPT | CTRL),
        "meh" => Some(SHIFT | OPT | CTRL),
        _ => None,
    }
}

/// Modifier symbols in macOS menu order: ⌃ ⌥ ⇧ ⌘, then 🌐 for fn.
pub const FN_SYMBOL: &str = "🌐";

pub fn mod_symbols(m: u8) -> Vec<&'static str> {
    let mut v = Vec::new();
    if m & CTRL != 0 { v.push("⌃"); }
    if m & OPT != 0 { v.push("⌥"); }
    if m & SHIFT != 0 { v.push("⇧"); }
    if m & CMD != 0 { v.push("⌘"); }
    if m & FN != 0 { v.push(FN_SYMBOL); }
    v
}

pub fn mod_words(m: u8) -> &'static str {
    match m {
        CMD => "Command",
        SHIFT => "Shift",
        OPT => "Option",
        CTRL => "Control",
        FN => "fn",
        _ => "modifiers",
    }
}

pub fn key_symbol(key: &str) -> String {
    match key {
        "Left" => "←".into(),
        "Right" => "→".into(),
        "Up" => "↑".into(),
        "Down" => "↓".into(),
        "Return" => "↩".into(),
        "Enter" => "⌤".into(),
        "Tab" => "⇥".into(),
        "Delete" => "⌫".into(),
        "Forward Delete" => "⌦".into(),
        "Escape" => "⎋".into(),
        "Page Up" => "⇞".into(),
        "Page Down" => "⇟".into(),
        "Home" => "↖".into(),
        "End" => "↘".into(),
        _ => key.to_string(),
    }
}

/// The text on a key cap. Page Up, Page Down, Home and End read as words: their macOS symbols
/// (⇞ ⇟ ↖ ↘) are too small to read at list size. Stored text keeps the symbols (`key_symbol`).
pub fn cap_label(key: &str) -> String {
    match key {
        "Page Up" => "PgUp".into(),
        "Page Down" => "PgDn".into(),
        "Home" | "End" => key.into(),
        _ => key_symbol(key),
    }
}

/// The parts of a trigger as key caps (modifier symbols then the key).
pub fn trigger_caps(t: &Trigger) -> (Vec<String>, String) {
    match t {
        Trigger::Combo { mods, key } => (
            mod_symbols(*mods).into_iter().map(String::from).collect(),
            cap_label(key),
        ),
        Trigger::ModTap { mods, taps } => {
            let sym = mod_symbols(*mods).join("");
            let n = (*taps).max(1) as usize;
            let note = match n {
                1 => "press".to_string(),
                2 => "double-tap".to_string(),
                3 => "triple-tap".to_string(),
                _ => format!("{n}× tap"),
            };
            (vec![sym; n], note)
        }
        Trigger::ModHold { mods } => (vec![mod_symbols(*mods).join("")], "hold".into()),
    }
}

/// Plain text, for search and sorting: "⌃⌥⌘←", "⌥ ⌥ double-tap".
pub fn trigger_text(t: &Trigger) -> String {
    match t {
        Trigger::Combo { mods, key } => format!("{}{}", mod_symbols(*mods).join(""), key_symbol(key)),
        _ => {
            let (caps, note) = trigger_caps(t);
            format!("{} {}", caps.join(" "), note)
        }
    }
}

/// Words for searching: "cmd shift t", "control option command left".
pub fn trigger_words(t: &Trigger) -> String {
    let m = t.mods();
    let mut w = Vec::new();
    if m & FN != 0 { w.push("fn globe"); }
    if m & CTRL != 0 { w.push("ctrl control"); }
    if m & OPT != 0 { w.push("opt option alt"); }
    if m & SHIFT != 0 { w.push("shift"); }
    if m & CMD != 0 { w.push("cmd command"); }
    let tail = match t {
        Trigger::Combo { key, .. } => key.to_lowercase(),
        Trigger::ModTap { taps, .. } => if *taps == 2 { "double tap twice".into() } else { "tap press".into() },
        Trigger::ModHold { .. } => "hold".into(),
    };
    format!("{} {}", w.join(" "), tail)
}

/// What a typed lookup holds: the keys it names, the words that are not keys, and the result.
struct Typed {
    trigger: Option<Trigger>,
    modifiers_only: bool,
    /// Words that are not a modifier, "double", "hold", or a known key name.
    unknown: Vec<String>,
    /// Every key the text names. More than one is an error in the lookup.
    keys: Vec<String>,
}

/// A key name the lookup understands: one character, a named key, F1-F20, or a keypad key.
pub fn is_key_word(raw: &str) -> bool {
    let t = raw.trim();
    if t.chars().count() == 1 {
        return true;
    }
    let low: String = t.to_lowercase().chars().filter(|c| !c.is_whitespace() && *c != '_').collect();
    if low.strip_prefix('f').is_some_and(|n| !n.is_empty() && n.len() <= 2 && n.chars().all(|c| c.is_ascii_digit())) {
        return true;
    }
    if low.starts_with("keypad") || low.strip_prefix("num").is_some_and(|n| n.len() == 1 && n.chars().all(|c| c.is_ascii_digit())) {
        return true;
    }
    // A named key changes its spelling in `normalize_key`, or is one of the names that stay the same.
    let mut c = t.chars();
    let title = c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default();
    let n = normalize_key(t);
    n != title || ["Left", "Right", "Up", "Down", "Return", "Space", "Tab", "Escape", "Delete", "Home", "End", "Help"].contains(&n.as_str())
}

fn scan_typed(input: &str) -> Typed {
    let mut out = Typed { trigger: None, modifiers_only: false, unknown: Vec::new(), keys: Vec::new() };
    let s = input.trim();
    if s.is_empty() {
        return out;
    }
    let mut mods = 0u8;
    let mut key: Option<String> = None;
    let mut taps = 0u8;
    let mut hold = false;
    // Symbols first.
    let mut rest = String::new();
    for c in s.chars() {
        let sym = match c {
            '⌘' => {
                mods |= CMD;
                None
            }
            '⇧' => {
                mods |= SHIFT;
                None
            }
            '⌥' => {
                mods |= OPT;
                None
            }
            '⌃' => {
                mods |= CTRL;
                None
            }
            '←' => Some("Left"),
            '→' => Some("Right"),
            '↑' => Some("Up"),
            '↓' => Some("Down"),
            '↩' => Some("Return"),
            '⇥' => Some("Tab"),
            '⌫' => Some("Delete"),
            '⎋' => Some("Escape"),
            _ => {
                rest.push(c);
                None
            }
        };
        if let Some(k) = sym {
            key = Some(k.into());
            out.keys.push(k.into());
        }
    }
    // "ctrl+alt+left" and "ctrl-alt-left" join words; a "+" or "-" that stands by itself or ends a word is the key.
    let mut words: Vec<String> = Vec::new();
    for tok in rest.split_whitespace() {
        if tok == "+" || tok == "-" {
            words.push(tok.to_string());
            continue;
        }
        let (body, last) = match tok.chars().last() {
            Some(c @ ('+' | '-')) => (&tok[..tok.len() - 1], Some(c)),
            _ => (tok, None),
        };
        words.extend(body.split(['+', '-']).filter(|w| !w.is_empty()).map(String::from));
        words.extend(last.map(|c| c.to_string()));
    }
    for word in &words {
        let lw = word.to_lowercase();
        match lw.as_str() {
            "double" | "twice" | "double-tap" | "doubletap" => taps = 2,
            "triple" => taps = 3,
            "tap" | "press" => taps = taps.max(1),
            "hold" => hold = true,
            _ => {
                if let Some(m) = modifier_word(&lw) {
                    mods |= m;
                } else {
                    if !is_key_word(word) {
                        out.unknown.push(word.clone());
                    }
                    key = Some(normalize_key(word));
                    out.keys.push(word.clone());
                }
            }
        }
    }
    out.modifiers_only = key.is_none() && mods != 0 && !hold && taps == 0;
    out.trigger = if let Some(k) = key {
        Some(clean_trigger(mods, k))
    } else if mods == 0 {
        None
    } else if hold {
        Some(Trigger::ModHold { mods })
    } else {
        Some(Trigger::ModTap { mods, taps: if taps == 0 { 2 } else { taps } })
    };
    out
}

/// Parse a typed lookup such as "cmd shift t", "⌘⇧T", "double option", "ctrl+alt+left".
/// Lenient: a word it does not know becomes the key. Use `check_typed` where a wrong guess misleads.
pub fn parse_typed(input: &str) -> Option<Trigger> {
    scan_typed(input).trigger
}

/// Parse a typed lookup strictly. The error is a sentence for the person, and empty for empty input.
pub fn check_typed(input: &str) -> Result<Trigger, String> {
    checked_input(input).map(|(trigger, _)| trigger)
}

#[derive(Debug, PartialEq, Eq)]
pub enum LookupQuery {
    Shortcut(Trigger),
    Modifiers(u8),
}

impl LookupQuery {
    pub fn matches(&self, trigger: &Trigger) -> bool {
        match self {
            Self::Shortcut(exact) => exact == trigger,
            Self::Modifiers(mods) => trigger.bare_modifier() == Some(*mods),
        }
    }
}

pub fn check_lookup(input: &str) -> Result<LookupQuery, String> {
    let (trigger, modifiers_only) = checked_input(input)?;
    Ok(if modifiers_only { LookupQuery::Modifiers(trigger.mods()) } else { LookupQuery::Shortcut(trigger) })
}

fn checked_input(input: &str) -> Result<(Trigger, bool), String> {
    if input.trim().is_empty() {
        return Err(String::new());
    }
    let t = scan_typed(input);
    if !t.unknown.is_empty() {
        let words: Vec<String> = t.unknown.iter().map(|w| format!("\"{w}\"")).collect();
        return Err(format!("Keyclash cannot read {}.", words.join(" and ")));
    }
    if t.keys.len() > 1 {
        return Err(format!("A shortcut has one key. This text names {}.", t.keys.join(" and ")));
    }
    t.trigger.map(|trigger| (trigger, t.modifiers_only))
        .ok_or_else(|| "Add a key or a modifier key, for example cmd t or double option.".to_string())
}

pub fn modifier_text(mods: u8) -> String {
    [(CTRL, "ctrl"), (OPT, "opt"), (SHIFT, "shift"), (CMD, "cmd"), (FN, "fn")]
        .into_iter().filter_map(|(bit, word)| (mods & bit != 0).then_some(word)).collect::<Vec<_>>().join(" ")
}

/// The words to type for a trigger in the lookup box: "shift cmd t", "double opt", "hold fn".
pub fn typed_text(t: &Trigger) -> String {
    let mods = modifier_text(t.mods());
    match t {
        Trigger::Combo { key, .. } => {
            let key: String = key.to_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
            if mods.is_empty() { key } else { format!("{mods} {key}") }
        }
        Trigger::ModTap { taps, .. } => {
            let gesture = match taps { 1 => "press", 2 | 0 => "double", _ => "triple" };
            format!("{gesture} {mods}")
        }
        Trigger::ModHold { .. } => format!("hold {mods}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typed() {
        assert_eq!(parse_typed("cmd shift t"), Some(Trigger::combo(CMD | SHIFT, "T")));
        assert_eq!(parse_typed("⌘⇧T"), Some(Trigger::combo(CMD | SHIFT, "T")));
        assert_eq!(parse_typed("double option"), Some(Trigger::ModTap { mods: OPT, taps: 2 }));
        assert_eq!(parse_typed("ctrl+alt+left"), Some(Trigger::combo(CTRL | OPT, "Left")));
        assert_eq!(parse_typed("ctrl-alt-left"), Some(Trigger::combo(CTRL | OPT, "Left")));
        assert_eq!(parse_typed("cmd -"), Some(Trigger::combo(CMD, "-")));
        assert_eq!(parse_typed("cmd-"), Some(Trigger::combo(CMD, "-")));
        assert_eq!(parse_typed("cmd +"), Some(Trigger::combo(CMD | SHIFT, "=")));
        assert_eq!(parse_typed("cmd ?"), Some(Trigger::combo(CMD | SHIFT, "/")));
        assert_eq!(parse_typed("cmd Questionmark"), Some(Trigger::combo(CMD | SHIFT, "/")));
        assert_eq!(parse_typed("hold fn"), Some(Trigger::ModHold { mods: FN }));
    }

    #[test]
    fn a_typo_is_not_read_as_a_key() {
        // "shfit" is not a modifier: the lenient parser made it the key, then "t" won (⌘T).
        assert_eq!(parse_typed("cmd shfit t"), Some(Trigger::combo(CMD, "T")));
        let e = check_typed("cmd shfit t").unwrap_err();
        assert!(e.contains("\"shfit\""), "{e}");
        assert!(check_typed("cmd t a").unwrap_err().contains("one key"));
        assert_eq!(check_typed("  "), Err(String::new()));
        assert_eq!(check_typed("cmd shift t"), Ok(Trigger::combo(CMD | SHIFT, "T")));
        assert_eq!(check_typed("⌃⌥⌘←"), Ok(Trigger::combo(CTRL | OPT | CMD, "Left")));
        assert_eq!(check_typed("cmd -"), Ok(Trigger::combo(CMD, "-")));
        assert_eq!(check_typed("opt f5"), Ok(Trigger::combo(OPT, "F5")));
        assert_eq!(check_typed("ctrl arrowleft"), Ok(Trigger::combo(CTRL, "Left")));
        assert_eq!(check_typed("cmd pagedown"), Ok(Trigger::combo(CMD, "Page Down")));
    }

    #[test]
    fn bare_modifier_lookup_matches_only_the_selected_modifier_set() {
        let query = check_lookup("ctrl").unwrap();
        let tap = Trigger::ModTap { mods: CTRL, taps: 1 };
        let hold = Trigger::ModHold { mods: CTRL };
        assert_eq!(query, LookupQuery::Modifiers(CTRL));
        assert!(query.matches(&tap));
        assert!(query.matches(&hold));
        assert!(!query.matches(&Trigger::ModHold { mods: CTRL | FN }));
        assert!(!query.matches(&Trigger::combo(CTRL, "C")));
        assert!(check_lookup("press ctrl").unwrap().matches(&tap));
        assert!(!check_lookup("press ctrl").unwrap().matches(&hold));
    }

    #[test]
    fn typed_text_reads_back() {
        for t in [
            Trigger::combo(CMD | SHIFT, "T"),
            Trigger::combo(CTRL | OPT | CMD, "Left"),
            Trigger::combo(CMD, "Space"),
            Trigger::combo(CMD, ","),
            Trigger::combo(CMD, "-"),
            Trigger::combo(OPT, "Page Down"),
            Trigger::combo(CTRL | FN, "F"),
            Trigger::ModTap { mods: CTRL, taps: 1 },
            Trigger::ModTap { mods: OPT, taps: 2 },
            Trigger::ModHold { mods: FN },
        ] {
            assert_eq!(check_typed(&typed_text(&t)), Ok(t.clone()), "{}", typed_text(&t));
        }
    }

    #[test]
    fn page_keys_have_readable_caps() {
        assert_eq!(trigger_caps(&Trigger::combo(0, "Page Down")).1, "PgDn");
        assert_eq!(trigger_caps(&Trigger::combo(CMD, "Home")).1, "Home");
        // The stored text keeps the macOS symbol, so stars and counts keep their keys.
        assert_eq!(trigger_text(&Trigger::combo(0, "Page Down")), "⇟");
    }

    #[test]
    fn one_spelling_per_chord() {
        assert_eq!(clean_trigger(CMD, "?".into()), Trigger::combo(CMD | SHIFT, "/"));
        assert_eq!(clean_trigger(CMD | SHIFT, "/".into()), Trigger::combo(CMD | SHIFT, "/"));
        assert_eq!(clean_trigger(CMD, "{".into()), Trigger::combo(CMD | SHIFT, "["));
        assert_eq!(clean_trigger(CMD, "T".into()), Trigger::combo(CMD, "T"));
    }

    #[test]
    fn shift_follows_the_layout() {
        // Part of a German layout: "+" and "#" have their own keys; "*" is Shift with "+", "?" is Shift with "ß".
        let map = |pairs: &[(i64, &str)]| pairs.iter().map(|&(c, k)| (c, k.to_string())).collect();
        let de = crate::layout::Layout {
            id: "test.de".into(),
            name: "German".into(),
            keys: map(&[(30, "+"), (42, "#"), (27, "ß"), (20, "3")]),
            shifted: map(&[(30, "*"), (42, "'"), (27, "?"), (20, "§")]),
        };
        let de = Some(&de);
        assert_eq!(shifted_base_in("+", de), None, "⌘+ is one key in German");
        assert_eq!(shifted_base_in("#", de), None, "⌘# is not ⇧⌘3 in German");
        assert_eq!(shifted_base_in("*", de).as_deref(), Some("+"));
        assert_eq!(shifted_base_in("?", de).as_deref(), Some("ß"));
        // A character the layout cannot type keeps the US rule.
        assert_eq!(shifted_base_in("{", de).as_deref(), Some("["));
        // No layout, or a layout read without Shift data: US.
        assert_eq!(shifted_base_in("+", None).as_deref(), Some("="));
        assert_eq!(shifted_base_in("T", None), None);
    }

    #[test]
    fn ax_letters_do_not_mean_shift() {
        // Accessibility gives "A" for ⌘A; a nib file gives "a" for ⌘A and "A" for ⇧⌘A.
        assert_eq!(key_from_ax_char("A"), Some(("A".into(), false)));
        assert_eq!(key_from_char("A"), Some(("A".into(), true)));
        assert_eq!(key_from_char("a"), Some(("A".into(), false)));
        assert_eq!(key_from_ax_char("\u{19}"), Some(("Tab".into(), true)));
        assert_eq!(key_from_char("🎤"), None);
    }

    #[test]
    fn ax_menu_bits() {
        assert_eq!(mods_from_ax_menu(0), CMD);
        assert_eq!(mods_from_ax_menu(1), CMD | SHIFT);
        assert_eq!(mods_from_ax_menu(8), 0);
        assert_eq!(mods_from_ax_menu(6), CMD | OPT | CTRL);
        assert_eq!(mods_from_ax_menu(16), CMD);
        assert_eq!(mods_from_ax_menu(24), FN);
        assert_eq!(modifier_word("lalt"), Some(OPT));
        assert_eq!(modifier_word("hyper"), Some(CMD | SHIFT | OPT | CTRL));
    }

    #[test]
    fn nsevent_flags() {
        assert_eq!(mods_from_nsevent(1572864), OPT | CMD);
        assert_eq!(mods_from_nsevent(786432), CTRL | OPT);
        assert_eq!(mods_from_nsevent(1179648), SHIFT | CMD);
    }
}
