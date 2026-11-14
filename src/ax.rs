//! Live menus of running apps, read through the macOS Accessibility API:
//! menu bar -> menus -> items, with AXMenuItemCmdChar / Modifiers / VirtualKey / Glyph
//! fetched in one call per item (four attributes).

#![allow(non_upper_case_globals)]

use std::ffi::{c_void, CString};
use std::ptr;

type CFTypeRef = *const c_void;
type CFStringRef = *const c_void;
type CFArrayRef = *const c_void;
type CFIndex = isize;

#[repr(C)]
struct CFArrayCallBacks {
    _private: [u8; 0],
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(e: CFTypeRef, attr: CFStringRef, out: *mut CFTypeRef) -> i32;
    fn AXUIElementCopyMultipleAttributeValues(e: CFTypeRef, attrs: CFArrayRef, options: u32, out: *mut CFArrayRef) -> i32;
    fn AXUIElementSetMessagingTimeout(e: CFTypeRef, seconds: f32) -> i32;
    fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> bool;
    fn AXUIElementPerformAction(e: CFTypeRef, action: CFStringRef) -> i32;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFTypeArrayCallBacks: CFArrayCallBacks;
    static kCFTypeDictionaryKeyCallBacks: CFArrayCallBacks;
    static kCFTypeDictionaryValueCallBacks: CFArrayCallBacks;
    static kCFBooleanTrue: CFTypeRef;
    fn CFDictionaryCreate(
        alloc: CFTypeRef,
        keys: *const CFTypeRef,
        values: *const CFTypeRef,
        n: CFIndex,
        kcb: *const CFArrayCallBacks,
        vcb: *const CFArrayCallBacks,
    ) -> CFTypeRef;
    fn CFRelease(cf: CFTypeRef);
    fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    fn CFGetTypeID(cf: CFTypeRef) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFArrayGetCount(a: CFArrayRef) -> CFIndex;
    fn CFArrayGetValueAtIndex(a: CFArrayRef, i: CFIndex) -> CFTypeRef;
    fn CFArrayCreate(alloc: CFTypeRef, values: *const CFTypeRef, n: CFIndex, cb: *const CFArrayCallBacks) -> CFArrayRef;
    fn CFStringCreateWithCString(alloc: CFTypeRef, s: *const i8, enc: u32) -> CFStringRef;
    fn CFStringGetLength(s: CFStringRef) -> CFIndex;
    fn CFStringGetCString(s: CFStringRef, buf: *mut i8, size: CFIndex, enc: u32) -> bool;
    fn CFNumberGetValue(n: CFTypeRef, kind: isize, out: *mut c_void) -> bool;
}

const UTF8: u32 = 0x0800_0100;

pub fn is_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Ask macOS for Accessibility permission. macOS shows its own dialog and adds
/// this app to Privacy & Security › Accessibility, where the person turns it on.
pub fn request_trust() -> bool {
    unsafe {
        let keys = [kAXTrustedCheckOptionPrompt];
        let values = [kCFBooleanTrue];
        let opts = Cf(CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        ));
        AXIsProcessTrustedWithOptions(opts.0)
    }
}

struct Cf(CFTypeRef);
impl Drop for Cf {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) }
        }
    }
}

fn cfstr(s: &str) -> Cf {
    let c = CString::new(s).unwrap();
    Cf(unsafe { CFStringCreateWithCString(ptr::null(), c.as_ptr(), UTF8) })
}

unsafe fn to_string(v: CFTypeRef) -> Option<String> {
    unsafe {
        if v.is_null() || CFGetTypeID(v) != CFStringGetTypeID() {
            return None;
        }
        let len = CFStringGetLength(v);
        let cap = len * 4 + 1;
        let mut buf = vec![0i8; cap as usize];
        if !CFStringGetCString(v, buf.as_mut_ptr(), cap, UTF8) {
            return None;
        }
        let bytes: Vec<u8> = buf.iter().take_while(|&&b| b != 0).map(|&b| b as u8).collect();
        String::from_utf8(bytes).ok()
    }
}

unsafe fn to_i64(v: CFTypeRef) -> Option<i64> {
    unsafe {
        if v.is_null() || CFGetTypeID(v) != CFNumberGetTypeID() {
            return None;
        }
        let mut out: i64 = 0;
        // kCFNumberSInt64Type = 4
        CFNumberGetValue(v, 4, &mut out as *mut i64 as *mut c_void).then_some(out)
    }
}

/// Children array of an element, retained by the caller-owned `Cf`.
unsafe fn children(e: CFTypeRef, attr: &Cf) -> Option<Cf> {
    unsafe {
        let mut out: CFTypeRef = ptr::null();
        if AXUIElementCopyAttributeValue(e, attr.0, &mut out) != 0 || out.is_null() {
            return None;
        }
        let v = Cf(out);
        if CFGetTypeID(v.0) != CFArrayGetTypeID() {
            return None;
        }
        Some(v)
    }
}

#[derive(Debug, Clone)]
pub struct LiveItem {
    pub path: String,
    pub title: String,
    pub cmd_char: Option<String>,
    pub modifiers: Option<i64>,
    pub virtual_key: Option<i64>,
    pub glyph: Option<i64>,
}

struct Walker {
    a_children: Cf,
    multi: Cf,
    items: Vec<LiveItem>,
    budget: usize,
}

const ATTRS: [&str; 6] = [
    "AXTitle",
    "AXMenuItemCmdChar",
    "AXMenuItemCmdModifiers",
    "AXMenuItemCmdVirtualKey",
    "AXMenuItemCmdGlyph",
    "AXChildren",
];

impl Walker {
    unsafe fn walk_menu(&mut self, menu: CFTypeRef, path: &str, depth: usize) {
        unsafe {
            if depth > 5 || self.budget == 0 {
                return;
            }
            let Some(kids) = children(menu, &self.a_children) else { return };
            let n = CFArrayGetCount(kids.0);
            for i in 0..n {
                if self.budget == 0 {
                    return;
                }
                self.budget -= 1;
                let item = CFArrayGetValueAtIndex(kids.0, i);
                let mut vals: CFArrayRef = ptr::null();
                if AXUIElementCopyMultipleAttributeValues(item, self.multi.0, 0, &mut vals) != 0 || vals.is_null() {
                    continue;
                }
                let vals = Cf(vals);
                let g = |k: isize| CFArrayGetValueAtIndex(vals.0, k);
                let title = to_string(g(0)).unwrap_or_default();
                let cmd_char = to_string(g(1)).filter(|s| !s.is_empty());
                let modifiers = to_i64(g(2));
                let virtual_key = to_i64(g(3));
                let glyph = to_i64(g(4)).filter(|&x| x != 0);
                if cmd_char.is_some() || glyph.is_some() {
                    self.items.push(LiveItem {
                        path: path.to_string(),
                        title: title.clone(),
                        cmd_char,
                        modifiers,
                        virtual_key,
                        glyph,
                    });
                }
                // Submenu: AXChildren holds one AXMenu.
                let sub = g(5);
                if !sub.is_null() && CFGetTypeID(sub) == CFArrayGetTypeID() && CFArrayGetCount(sub) > 0 {
                    let child_path = if path.is_empty() { title.clone() } else { format!("{path} › {title}") };
                    let n2 = CFArrayGetCount(sub);
                    for j in 0..n2 {
                        let m = CFRetain(CFArrayGetValueAtIndex(sub, j));
                        let m = Cf(m);
                        self.walk_menu(m.0, &child_path, depth + 1);
                    }
                }
            }
        }
    }
}

/// Read every menu item with a shortcut in a running app.
pub fn read_app_menus(pid: i32, max_items: usize, per_menu_limit: Option<usize>) -> Option<Vec<LiveItem>> {
    unsafe {
        let app = Cf(AXUIElementCreateApplication(pid));
        if app.0.is_null() {
            return None;
        }
        AXUIElementSetMessagingTimeout(app.0, 1.5);
        let menubar_attr = cfstr("AXMenuBar");
        let mut bar: CFTypeRef = ptr::null();
        if AXUIElementCopyAttributeValue(app.0, menubar_attr.0, &mut bar) != 0 || bar.is_null() {
            return None;
        }
        let bar = Cf(bar);
        let attr_strings: Vec<Cf> = ATTRS.iter().map(|a| cfstr(a)).collect();
        let raw: Vec<CFTypeRef> = attr_strings.iter().map(|c| c.0).collect();
        let multi = Cf(CFArrayCreate(ptr::null(), raw.as_ptr(), raw.len() as CFIndex, &kCFTypeArrayCallBacks));
        let mut w = Walker { a_children: cfstr("AXChildren"), multi, items: Vec::new(), budget: max_items };
        let title_attr = cfstr("AXTitle");
        let top = children(bar.0, &w.a_children)?;
        let n = CFArrayGetCount(top.0);
        // Index 0 is the Apple menu, the same in every app.
        for i in 1..n {
            let top_item = CFArrayGetValueAtIndex(top.0, i);
            let mut t: CFTypeRef = ptr::null();
            let title = if AXUIElementCopyAttributeValue(top_item, title_attr.0, &mut t) == 0 {
                let t = Cf(t);
                to_string(t.0).unwrap_or_default()
            } else {
                String::new()
            };
            let Some(menus) = children(top_item, &w.a_children) else { continue };
            // Browsers: at most `per_menu_limit` items per top-level menu.
            let total_left = w.budget;
            if let Some(limit) = per_menu_limit {
                w.budget = w.budget.min(limit);
            }
            let before = w.budget;
            for j in 0..CFArrayGetCount(menus.0) {
                let m = CFArrayGetValueAtIndex(menus.0, j);
                w.walk_menu(m, &title, 0);
            }
            w.budget = total_left.saturating_sub(before - w.budget);
        }
        Some(w.items)
    }
}

/// Run a menu item of a running app, as a click on it would: walk the menu bar by titles
/// ("Edit", "Find", "Find Next…") and press the last one. False when an item is not found
/// (the menu changed since it was read) or the app refuses.
pub fn press_menu_item(pid: i32, path: &[String]) -> bool {
    if pid <= 0 || path.is_empty() {
        return false;
    }
    unsafe {
        let app = Cf(AXUIElementCreateApplication(pid));
        if app.0.is_null() {
            return false;
        }
        AXUIElementSetMessagingTimeout(app.0, 1.5);
        let mut bar: CFTypeRef = ptr::null();
        if AXUIElementCopyAttributeValue(app.0, cfstr("AXMenuBar").0, &mut bar) != 0 || bar.is_null() {
            return false;
        }
        let bar = Cf(bar);
        let kids_attr = cfstr("AXChildren");
        let title_attr = cfstr("AXTitle");
        // The element to search in: the menu bar, then each open menu.
        let mut parent = Cf(CFRetain(bar.0));
        for (depth, want) in path.iter().enumerate() {
            let Some(kids) = children(parent.0, &kids_attr) else { return false };
            let mut found: Option<Cf> = None;
            // Index 0 of the menu bar is the Apple menu: the app menu is index 1 and has the app's name.
            for i in 0..CFArrayGetCount(kids.0) {
                let item = CFArrayGetValueAtIndex(kids.0, i);
                let mut t: CFTypeRef = ptr::null();
                if AXUIElementCopyAttributeValue(item, title_attr.0, &mut t) != 0 {
                    continue;
                }
                let t = Cf(t);
                if to_string(t.0).is_some_and(|s| s.trim() == want.trim()) {
                    found = Some(Cf(CFRetain(item)));
                    break;
                }
            }
            let Some(item) = found else { return false };
            if depth + 1 == path.len() {
                return AXUIElementPerformAction(item.0, cfstr("AXPress").0) == 0;
            }
            // A menu bar item or a submenu item holds one AXMenu: search in it next.
            let Some(menus) = children(item.0, &kids_attr) else { return false };
            if CFArrayGetCount(menus.0) == 0 {
                return false;
            }
            parent = Cf(CFRetain(CFArrayGetValueAtIndex(menus.0, 0)));
        }
        false
    }
}
