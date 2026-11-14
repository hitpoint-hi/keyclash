//! The macOS keyboard layout: which character each physical key gives.
//!
//! A menu shortcut follows the character ("⌘F" is the key that types F in the active layout).
//! A hotkey stored as a key code follows the physical key. With the layout known, the keyboard
//! map can label each physical key with its character, and a key-code hotkey gets the character
//! that its key types now.
//!
//! The macOS calls here (Text Input Sources) must run on the main thread.

#![allow(non_upper_case_globals)]

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::RwLock;

type CFTypeRef = *const c_void;
type CFStringRef = *const c_void;
type CFArrayRef = *const c_void;
type CFIndex = isize;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    static kTISPropertyUnicodeKeyLayoutData: CFStringRef;
    static kTISPropertyInputSourceID: CFStringRef;
    static kTISPropertyLocalizedName: CFStringRef;
    static kTISPropertyInputSourceType: CFStringRef;
    static kTISTypeKeyboardLayout: CFStringRef;
    fn TISCopyCurrentKeyboardLayoutInputSource() -> CFTypeRef;
    /// Follows the "get" rule: the result is not owned by the caller.
    fn TISGetInputSourceProperty(source: CFTypeRef, key: CFStringRef) -> CFTypeRef;
    fn TISCreateInputSourceList(properties: CFTypeRef, include_all_installed: u8) -> CFArrayRef;
    fn UCKeyTranslate(
        layout: *const u8,
        key_code: u16,
        action: u16,
        modifiers: u32,
        keyboard_type: u32,
        options: u32,
        dead_key_state: *mut u32,
        max_len: usize,
        len: *mut usize,
        out: *mut u16,
    ) -> i32;
    fn LMGetKbdType() -> u8;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: CFTypeRef);
    fn CFArrayGetCount(a: CFArrayRef) -> CFIndex;
    fn CFArrayGetValueAtIndex(a: CFArrayRef, i: CFIndex) -> CFTypeRef;
    fn CFStringGetLength(s: CFStringRef) -> CFIndex;
    fn CFStringGetCString(s: CFStringRef, buf: *mut i8, size: CFIndex, enc: u32) -> bool;
    fn CFDataGetBytePtr(d: CFTypeRef) -> *const u8;
    fn CFEqual(a: CFTypeRef, b: CFTypeRef) -> u8;
}

const UTF8: u32 = 0x0800_0100;
/// kUCKeyActionDisplay: the character to show for a key, without a key-down or key-up.
const ACTION_DISPLAY: u16 = 3;
/// kUCKeyTranslateNoDeadKeysMask: a dead key gives its own character.
const NO_DEAD_KEYS: u32 = 1;

/// Key codes of the keys that type a character: letters, digits and punctuation (US-ANSI `kVK_ANSI_*`,
/// and 10 for the extra key of an ISO keyboard). Return, Tab, Space and the like are the same in every layout.
pub const CHARACTER_KEYS: [i64; 48] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34,
    35, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 50,
];

/// One keyboard layout: the key name on each character key, by key code.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    /// For example "com.apple.keylayout.US".
    pub id: String,
    /// The name macOS shows in the input menu.
    pub name: String,
    pub keys: HashMap<i64, String>,
    /// The character each key types with Shift, when it differs from `keys` ("=" gives "+" in US).
    pub shifted: HashMap<i64, String>,
}

/// The key name for the character that a key types without modifier keys ("f" gives "F").
/// None when the key types nothing that can be shown.
pub fn key_name(typed: &str) -> Option<String> {
    let mut chars = typed.chars();
    let c = chars.next()?;
    if chars.next().is_some() || c.is_whitespace() || c.is_control() {
        return None;
    }
    Some(c.to_uppercase().to_string())
}

unsafe fn string_of(s: CFStringRef) -> Option<String> {
    unsafe {
        if s.is_null() {
            return None;
        }
        let cap = CFStringGetLength(s) * 4 + 1;
        let mut buf = vec![0i8; cap as usize];
        if !CFStringGetCString(s, buf.as_mut_ptr(), cap, UTF8) {
            return None;
        }
        let bytes: Vec<u8> = buf.iter().take_while(|&&b| b != 0).map(|&b| b as u8).collect();
        String::from_utf8(bytes).ok()
    }
}

/// Read a layout from a macOS input source. None for an input source without key data (an input method).
unsafe fn read(source: CFTypeRef) -> Option<Layout> {
    unsafe {
        let data = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData);
        if data.is_null() {
            return None;
        }
        let table = CFDataGetBytePtr(data);
        if table.is_null() {
            return None;
        }
        let keyboard = LMGetKbdType() as u32;
        let typed = |code: i64, modifiers: u32| -> Option<String> {
            let (mut dead, mut len, mut out) = (0u32, 0usize, [0u16; 4]);
            let err = UCKeyTranslate(table, code as u16, ACTION_DISPLAY, modifiers, keyboard, NO_DEAD_KEYS, &mut dead, out.len(), &mut len, out.as_mut_ptr());
            if err != 0 || len == 0 {
                return None;
            }
            key_name(&String::from_utf16_lossy(&out[..len.min(out.len())]))
        };
        let (mut keys, mut shifted) = (HashMap::new(), HashMap::new());
        for code in CHARACTER_KEYS {
            let Some(name) = typed(code, 0) else { continue };
            // UCKeyTranslate takes the Carbon modifier bits shifted right by 8: shiftKey (0x200) is 2.
            if let Some(up) = typed(code, 2).filter(|up| *up != name) {
                shifted.insert(code, up);
            }
            keys.insert(code, name);
        }
        Some(Layout {
            id: string_of(TISGetInputSourceProperty(source, kTISPropertyInputSourceID)).unwrap_or_default(),
            name: string_of(TISGetInputSourceProperty(source, kTISPropertyLocalizedName)).unwrap_or_default(),
            keys,
            shifted,
        })
    }
}

/// The id of the layout that is selected in macOS now. Cheap: use it to see a change.
pub fn current_id() -> Option<String> {
    unsafe {
        let source = TISCopyCurrentKeyboardLayoutInputSource();
        if source.is_null() {
            return None;
        }
        let id = string_of(TISGetInputSourceProperty(source, kTISPropertyInputSourceID));
        CFRelease(source);
        id
    }
}

/// The layout that is selected in macOS now.
pub fn read_current() -> Option<Layout> {
    unsafe {
        let source = TISCopyCurrentKeyboardLayoutInputSource();
        if source.is_null() {
            return None;
        }
        let layout = read(source);
        CFRelease(source);
        layout
    }
}

/// An installed layout by name or id, without selecting it ("Dvorak", "colemak dh").
/// A name that is equal wins over a name that only contains the text.
pub fn read_named(text: &str) -> Option<Layout> {
    let want = text.trim().to_lowercase();
    unsafe {
        let list = TISCreateInputSourceList(std::ptr::null(), 1);
        if list.is_null() {
            return None;
        }
        let mut partial = None;
        let mut exact = None;
        for i in 0..CFArrayGetCount(list) {
            let source = CFArrayGetValueAtIndex(list, i);
            let kind = TISGetInputSourceProperty(source, kTISPropertyInputSourceType);
            if kind.is_null() || CFEqual(kind, kTISTypeKeyboardLayout) == 0 {
                continue;
            }
            let name = string_of(TISGetInputSourceProperty(source, kTISPropertyLocalizedName)).unwrap_or_default().to_lowercase();
            let id = string_of(TISGetInputSourceProperty(source, kTISPropertyInputSourceID)).unwrap_or_default().to_lowercase();
            if name == want || id == want {
                exact = read(source);
                break;
            }
            if partial.is_none() && (name.contains(&want) || id.contains(&want)) {
                partial = read(source);
            }
        }
        CFRelease(list);
        exact.or(partial)
    }
}

// ------------------------------------------------------------ the layout in use

static ACTIVE: RwLock<Option<Layout>> = RwLock::new(None);

/// Use this layout for key names from now on. None means US, the layout of the key-code table.
pub fn set(layout: Option<Layout>) {
    if let Ok(mut a) = ACTIVE.write() {
        *a = layout;
    }
}

pub fn active_id() -> Option<String> {
    ACTIVE.read().ok()?.as_ref().map(|l| l.id.clone())
}

pub fn active_name() -> Option<String> {
    ACTIVE.read().ok()?.as_ref().map(|l| l.name.clone())
}

/// The key name on a character key in the layout in use. None for other keys, and when no layout is set.
pub fn key_at(code: i64) -> Option<String> {
    ACTIVE.read().ok()?.as_ref()?.keys.get(&code).cloned()
}

/// The key code of a character key by its US name ("E" gives 14).
pub fn us_code(us_key: &str) -> Option<i64> {
    CHARACTER_KEYS.into_iter().find(|&c| crate::keys::keycode_name(c) == Some(us_key))
}

/// The key name on the physical key that types `us_key` in the US layout.
/// The keyboard map is drawn as a US keyboard, so this gives the label of each of its keys.
pub fn key_on_us_position(us_key: &str, layout: Option<&Layout>) -> String {
    us_code(us_key).and_then(|c| layout?.keys.get(&c).cloned()).unwrap_or_else(|| us_key.to_string())
}

/// The opposite of `key_on_us_position`: the US name of the physical key that types `key` in `layout`.
/// Tools that name keys by their US place (AeroSpace, Karabiner) need this name. Other keys keep their name.
pub fn us_position_of(key: &str, layout: Option<&Layout>) -> String {
    layout
        .and_then(|l| CHARACTER_KEYS.into_iter().find(|c| l.keys.get(c).map(String::as_str) == Some(key)))
        .and_then(crate::keys::keycode_name)
        .map(str::to_string)
        .unwrap_or_else(|| key.to_string())
}

/// Run `f` with the layout in use (None: US).
pub fn with_active<R>(f: impl FnOnce(Option<&Layout>) -> R) -> R {
    match ACTIVE.read() {
        Ok(a) => f(a.as_ref()),
        Err(_) => f(None),
    }
}

/// `key_on_us_position` for the layout in use.
pub fn key_on(us_key: &str) -> String {
    with_active(|layout| key_on_us_position(us_key, layout))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Colemak-DH letters that differ from US, by key code.
    fn colemak_dh() -> Layout {
        let keys = [(14, "F"), (15, "P"), (17, "B"), (16, "J"), (32, "L"), (34, "U"), (31, "Y"), (35, ";"), (1, "R"), (2, "S"), (3, "T"), (4, "M"), (38, "N"), (40, "E"), (37, "I"), (41, "O")];
        Layout { id: "test.colemak-dh".into(), name: "Colemak DH".into(), keys: keys.into_iter().map(|(c, k)| (c, k.to_string())).collect(), shifted: HashMap::new() }
    }

    #[test]
    fn a_typed_character_gives_a_key_name() {
        assert_eq!(key_name("f").as_deref(), Some("F"));
        assert_eq!(key_name(";").as_deref(), Some(";"));
        assert_eq!(key_name("é").as_deref(), Some("É"));
        assert_eq!(key_name(""), None);
        assert_eq!(key_name(" "), None);
        assert_eq!(key_name("\u{10}"), None);
    }

    #[test]
    fn us_names_have_key_codes() {
        assert_eq!(us_code("E"), Some(14));
        assert_eq!(us_code(";"), Some(41));
        assert_eq!(us_code("`"), Some(50));
        // Not character keys.
        assert_eq!(us_code("Space"), None);
        assert_eq!(us_code("F5"), None);
    }

    #[test]
    fn the_map_shows_the_layout_on_us_positions() {
        let l = colemak_dh();
        // The key at the US "E" position types F in Colemak DH.
        assert_eq!(key_on_us_position("E", Some(&l)), "F");
        assert_eq!(key_on_us_position("P", Some(&l)), ";");
        // A key the layout does not change, and keys that are no character keys.
        assert_eq!(key_on_us_position("Q", Some(&l)), "Q");
        assert_eq!(key_on_us_position("Space", Some(&l)), "Space");
        // No layout: US.
        assert_eq!(key_on_us_position("E", None), "E");
    }

    #[test]
    fn key_writers_can_find_the_us_position_in_another_layout() {
        let l = colemak_dh();
        assert_eq!(us_position_of("F", Some(&l)), "E");
        assert_eq!(us_position_of(";", Some(&l)), "P");
        assert_eq!(us_position_of("Space", Some(&l)), "Space");
        assert_eq!(us_position_of("F", None), "F");
    }
}
