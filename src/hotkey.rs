//! Watch for a double press of the Command key anywhere on the Mac,
//! then tell the window which app was in front so the shortcuts panel can show its keys.
//!
//! The modifier state is polled with `CGEventSourceFlagsState`, so no event tap is needed.

use crate::keys::{NS_COMMAND, NS_CONTROL, NS_FN, NS_OPTION, NS_SHIFT};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

/// Longest gap between the two presses: 0.5 s, measured from the first press.
pub const KEY_BETWEEN_DELAY: f64 = 0.5;
/// 0.5 s: how long the second press is held
/// before the panel shows.
pub const KEY_HOLDING_DELAY: f64 = 0.5;
/// Default activation: press the key twice and hold.
#[allow(dead_code)]
pub const ACTIVATION_KEY_TYPE: i32 = 1;
/// Poll interval of the modifier state.
const POLL: Duration = Duration::from_millis(12);
/// Wait between checks while the event tap runs and no double press is in progress.
const IDLE_WAIT: Duration = Duration::from_millis(250);
/// How often the app in front is recorded as the fallback target.
const FRONT_APP_CHECK: Duration = Duration::from_secs(2);

const COMBINED_SESSION_STATE: i32 = 0;
const EVENT_KEY_DOWN: u32 = 10;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceFlagsState(state: i32) -> u64;
    fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
}

fn modifier_flags() -> i64 {
    let f = unsafe { CGEventSourceFlagsState(COMBINED_SESSION_STATE) } as i64;
    f & (NS_SHIFT | NS_CONTROL | NS_OPTION | NS_COMMAND | NS_FN)
}

/// Modifier keys held right now, for highlighting in the panel.
pub fn current_mods() -> u8 {
    crate::keys::mods_from_nsevent(modifier_flags())
}

fn seconds_since_key_down() -> f64 {
    unsafe { CGEventSourceSecondsSinceLastEventType(COMBINED_SESSION_STATE, EVENT_KEY_DOWN) }
}


#[derive(Clone, Debug)]
pub struct FrontApp {
    pub bundle: String,
    pub pid: i32,
    pub name: String,
    /// The app bundle on disk, for its icon. None when unknown.
    pub path: Option<String>,
}

pub enum HotkeyMsg {
    /// Command was pressed twice and released quickly: show (or hide) the panel; it stays open.
    Toggle(Option<FrontApp>),
    /// Command was pressed twice and held: show the panel while it is held.
    ShowHeld(Option<FrontApp>),
    /// All modifier keys were released after a held opening.
    ReleasedAll,
    /// Escape or another key was pressed while the panel is open. The keys, when they are a shortcut.
    KeyPressed(Option<crate::model::Trigger>),
}

/// The app in front, from `lsappinfo` (no permission needed).
pub fn front_app() -> Option<FrontApp> {
    let asn = Command::new("/usr/bin/lsappinfo").arg("front").output().ok()?;
    let asn = String::from_utf8_lossy(&asn.stdout).trim().to_string();
    if asn.is_empty() {
        return None;
    }
    let info = Command::new("/usr/bin/lsappinfo")
        .args(["info", &asn])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&info.stdout);
    parse_lsappinfo(&text)
}

/// An open app by its id, from `lsappinfo` (no permission needed). None when it is not open.
pub fn running_app(bundle: &str) -> Option<FrontApp> {
    let info = Command::new("/usr/bin/lsappinfo").args(["info", bundle]).output().ok()?;
    parse_lsappinfo(&String::from_utf8_lossy(&info.stdout))
}

/// `lsappinfo info` text: `"Safari" ASN:… (in front)`, `bundleID="com.apple.Safari"`, `pid = 69062 …`.
fn parse_lsappinfo(text: &str) -> Option<FrontApp> {
    let after = |key: &str| text.find(key).map(|i| &text[i + key.len()..]);
    let bundle = after("bundleID=\"").and_then(|r| r.split('"').next()).map(String::from);
    let pid = after("pid = ").and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next()).and_then(|d| d.parse().ok());
    let name = text.trim_start().strip_prefix('"').and_then(|r| r.split('"').next()).map(String::from);
    let path = after("bundle path=\"").and_then(|r| r.split('"').next()).filter(|p| p.ends_with(".app")).map(String::from);
    Some(FrontApp { bundle: bundle?, pid: pid?, name: name.unwrap_or_default(), path })
}

fn is_self(app: &FrontApp) -> bool {
    app.pid == std::process::id() as i32 || app.bundle == crate::bundle::BUNDLE_ID
}

pub fn should_return_focus(origin: &FrontApp) -> bool {
    !is_self(origin)
}

/// The front app counts unless it is this app or on the ignore list
/// (SystemUIServer, Control Center, Notification Center, Spotlight, … — sources::IGNORED);
/// otherwise the last app that counted is kept.
fn counts_as_target(app: &FrontApp) -> bool {
    !is_self(app) && !crate::sources::IGNORED.contains(&app.bundle.as_str())
}

/// When this app becomes active itself, it gives focus back to the other active app.
pub fn give_focus_back(app: &FrontApp) {
    if front_app().is_some_and(|f| is_self(&f)) {
        let _ = Command::new("/usr/bin/open").args(["-b", &app.bundle]).spawn();
    }
}

/// The key code that types `key` (the reverse of `keys::keycode_key`).
fn keycode_for(key: &str) -> Option<u16> {
    (0..128).find(|&c| crate::keys::keycode_key(c).as_deref() == Some(key)).map(|c| c as u16)
}

/// Send the keys of a shortcut, as if they were pressed. With a pid, only that app gets them;
/// without one, they go through the system, so a system-wide hotkey of another tool runs.
/// The use counter does not count them. Needs the Accessibility permission. False when the keys
/// cannot be typed (a modifier-only trigger, or a key with no key code).
pub fn post_keys(t: &crate::model::Trigger, pid: Option<i32>) -> bool {
    let crate::model::Trigger::Combo { mods, key } = t else { return false };
    let Some(code) = keycode_for(key) else { return false };
    let flags = crate::keys::mods_to_nsevent(*mods) as u64;
    for down in [true, false] {
        unsafe {
            let e = CGEventCreateKeyboardEvent(std::ptr::null(), code, down);
            if e.is_null() {
                return false;
            }
            CGEventSetFlags(e, flags);
            CGEventSetIntegerValueField(e, FIELD_USER_DATA, OWN_EVENT);
            match pid {
                Some(p) if p > 0 => CGEventPostToPid(p, e),
                _ => CGEventPost(0, e),
            }
            CFRelease(e);
        }
    }
    true
}

/// One line in the data folder, hotkey.log (kept short).
pub fn log(msg: &str) {
    use std::io::Write;
    let p = crate::sources::data_dir().join("hotkey.log");
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if std::fs::metadata(&p).map(|m| m.len() > 200_000).unwrap_or(false) {
        let _ = std::fs::remove_file(&p);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{} {msg}", crate::sources::now_secs());
    }
}

/// Log every modifier change (set by the file `debug-keys` next to the log).
static DEBUG_KEYS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// True while the key-event tap is running (needs Accessibility).
pub static TAP_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What the tap (or the poll) saw.
enum Obs {
    Flags(i64),
    /// A key press, with the shortcut it makes (None for plain typing).
    KeyDown(Option<crate::model::Trigger>),
    /// A mouse button went down. Only the event type is read: not the place, not the button.
    Click,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        mask: u64,
        callback: extern "C" fn(*const std::ffi::c_void, u32, *const std::ffi::c_void, *mut std::ffi::c_void) -> *const std::ffi::c_void,
        user: *mut std::ffi::c_void,
    ) -> *const std::ffi::c_void;
    fn CGEventTapEnable(tap: *const std::ffi::c_void, enable: bool);
    fn CGEventGetFlags(event: *const std::ffi::c_void) -> u64;
    fn CGEventGetIntegerValueField(event: *const std::ffi::c_void, field: u32) -> i64;
    fn CGEventSetIntegerValueField(event: *const std::ffi::c_void, field: u32, value: i64);
    fn CGEventCreateKeyboardEvent(source: *const std::ffi::c_void, key: u16, down: bool) -> *const std::ffi::c_void;
    fn CGEventSetFlags(event: *const std::ffi::c_void, flags: u64);
    fn CGEventPostToPid(pid: i32, event: *const std::ffi::c_void);
    fn CGEventPost(tap: u32, event: *const std::ffi::c_void);
    /// True when macOS lets this app read key events (Input Monitoring).
    fn CGPreflightListenEventAccess() -> bool;
}

/// True when macOS gives key events to this app's listener: the app has the Accessibility permission.
///
/// The tap is the standard type, not listen-only. A listen-only tap makes macOS ask for a second
/// permission (Input Monitoring). When that request is refused, macOS keeps a "denied" record, and then
/// no tap of this app gets key events, also with Accessibility on (seen on macOS 27 on 2026-11-09;
/// `tccutil reset ListenEvent <app id>` removes the record). So the app asks for one permission only,
/// and the callback gives each event back unchanged.
fn may_listen() -> bool {
    crate::ax::is_trusted()
}

/// `CGEventField` numbers: the key is a repeat of a held key, the key code, the process the key goes to.
const FIELD_AUTOREPEAT: u32 = 8;
const FIELD_KEYCODE: u32 = 9;
const FIELD_TARGET_PID: u32 = 40;
/// `kCGEventSourceUserData`: Keyclash marks the keys it sends itself, so the use counter skips them.
const FIELD_USER_DATA: u32 = 42;
const OWN_EVENT: i64 = 0x6B63_6C73; // "kcls"

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopCommonModes: *const std::ffi::c_void;
    fn CFMachPortCreateRunLoopSource(alloc: *const std::ffi::c_void, port: *const std::ffi::c_void, order: isize) -> *const std::ffi::c_void;
    fn CFRunLoopGetCurrent() -> *const std::ffi::c_void;
    fn CFRunLoopAddSource(rl: *const std::ffi::c_void, source: *const std::ffi::c_void, mode: *const std::ffi::c_void);
    fn CFRunLoopRun();
    fn CFRelease(cf: *const std::ffi::c_void);
}

/// CGEventTapCreate(…, mask 0xC00 key down/up) plus modifier changes (type 12).
const EVENT_FLAGS_CHANGED: u32 = 12;
/// Mouse-down events (left, right, other) tell a ⌘-click from a press of ⌘ alone.
const EVENT_LEFT_MOUSE_DOWN: u32 = 1;
const EVENT_RIGHT_MOUSE_DOWN: u32 = 3;
const EVENT_OTHER_MOUSE_DOWN: u32 = 25;
const TAP_MASK: u64 = (1 << EVENT_KEY_DOWN)
    | (1 << EVENT_FLAGS_CHANGED)
    | (1 << EVENT_LEFT_MOUSE_DOWN)
    | (1 << EVENT_RIGHT_MOUSE_DOWN)
    | (1 << EVENT_OTHER_MOUSE_DOWN);
const SESSION_TAP: u32 = 1;
const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_DISABLED_BY_USER: u32 = 0xFFFF_FFFF;

static TAP_PORT: std::sync::atomic::AtomicPtr<std::ffi::c_void> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
/// Events that the tap got since the app started: key presses, modifier changes. For the log.
static KEY_EVENTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static FLAG_EVENTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

extern "C" fn tap_callback(
    _proxy: *const std::ffi::c_void,
    etype: u32,
    event: *const std::ffi::c_void,
    user: *mut std::ffi::c_void,
) -> *const std::ffi::c_void {
    let tx = unsafe { &*(user as *const std::sync::mpsc::Sender<Obs>) };
    match etype {
        EVENT_FLAGS_CHANGED => {
            FLAG_EVENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let f = unsafe { CGEventGetFlags(event) } as i64;
            let _ = tx.send(Obs::Flags(f & (NS_SHIFT | NS_CONTROL | NS_OPTION | NS_COMMAND | NS_FN)));
        }
        EVENT_KEY_DOWN => {
            KEY_EVENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let own = unsafe { CGEventGetIntegerValueField(event, FIELD_USER_DATA) } == OWN_EVENT;
            let (code, flags) = unsafe { (CGEventGetIntegerValueField(event, FIELD_KEYCODE), CGEventGetFlags(event) as i64) };
            // The use counter: a held key counts one time. `usage::key_down` drops plain typing.
            if !own && crate::usage::counting() && unsafe { CGEventGetIntegerValueField(event, FIELD_AUTOREPEAT) } == 0 {
                let pid = unsafe { CGEventGetIntegerValueField(event, FIELD_TARGET_PID) };
                crate::usage::key_down(code, flags, pid as i32);
            }
            let _ = tx.send(Obs::KeyDown(crate::usage::trigger_of(code, flags)));
        }
        EVENT_LEFT_MOUSE_DOWN | EVENT_RIGHT_MOUSE_DOWN | EVENT_OTHER_MOUSE_DOWN => {
            let _ = tx.send(Obs::Click);
        }
        TAP_DISABLED_BY_TIMEOUT | TAP_DISABLED_BY_USER => {
            let port = TAP_PORT.load(std::sync::atomic::Ordering::Relaxed);
            if !port.is_null() {
                unsafe { CGEventTapEnable(port, true) };
            }
        }
        _ => {}
    }
    // Give every event back unchanged.
    event
}

/// Run the event tap on its own thread; retry every 3 s until macOS allows it.
fn start_tap(tx: std::sync::mpsc::Sender<Obs>) {
    std::thread::spawn(move || {
        let user = Box::into_raw(Box::new(tx)) as *mut std::ffi::c_void;
        let mut said_waiting = false;
        loop {
            let port = if may_listen() { unsafe { CGEventTapCreate(SESSION_TAP, 0, 0, TAP_MASK, tap_callback, user) } } else { std::ptr::null() };
            if !port.is_null() && !unsafe { CGPreflightListenEventAccess() } {
                // The tap exists but macOS will give it no keys: a "denied" Input Monitoring record.
                log("event tap: macOS refuses key events for this app (Input Monitoring is set to off in Privacy & Security)");
            }
            // One line per change, not one every 3 s.
            if !port.is_null() {
                log("event tap: running");
                said_waiting = false;
            } else if !said_waiting {
                log("event tap: not allowed yet (Accessibility off); trying again every 3 s");
                said_waiting = true;
            }
            if !port.is_null() {
                TAP_PORT.store(port as *mut _, std::sync::atomic::Ordering::Relaxed);
                TAP_ACTIVE.store(true, std::sync::atomic::Ordering::Relaxed);
                unsafe {
                    let src = CFMachPortCreateRunLoopSource(std::ptr::null(), port, 0);
                    CFRunLoopAddSource(CFRunLoopGetCurrent(), src, kCFRunLoopCommonModes);
                    CGEventTapEnable(port, true);
                    CFRunLoopRun();
                }
                TAP_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
            }
            std::thread::sleep(Duration::from_secs(3));
        }
    });
}

fn last_other() -> &'static std::sync::Mutex<Option<FrontApp>> {
    static L: std::sync::OnceLock<std::sync::Mutex<Option<FrontApp>>> = std::sync::OnceLock::new();
    L.get_or_init(Default::default)
}

/// The last app in front that is not Keyclash.
pub fn last_other_app() -> Option<FrontApp> {
    last_other().lock().ok().and_then(|g| g.clone())
}

/// The app the panel is for: the app in front now,
/// or the last app that counted when Keyclash or an ignored app is in front.
pub fn target_app() -> Option<FrontApp> {
    match front_app() {
        Some(f) if counts_as_target(&f) => {
            *last_other().lock().unwrap() = Some(f.clone());
            Some(f)
        }
        _ => last_other_app().or_else(|| {
            // Nothing recorded yet (Keyclash was in front since it started): take the app behind it.
            let f = first_visible_target()?;
            *last_other().lock().unwrap() = Some(f.clone());
            Some(f)
        }),
    }
}

/// `lsappinfo visibleProcessList` text, front to back: `ASN:0x0-0x19019-"Google_Chrome": ASN:…`.
fn visible_asns(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|t| t.starts_with("ASN:"))
        .filter_map(|t| t.split("-\"").next())
        .map(|a| format!("{a}:"))
        .collect()
}

/// The frontmost visible app that counts as a target. It walks the running
/// apps in the same way when the app in front is ignored.
fn first_visible_target() -> Option<FrontApp> {
    let out = Command::new("/usr/bin/lsappinfo").arg("visibleProcessList").output().ok()?;
    for asn in visible_asns(&String::from_utf8_lossy(&out.stdout)).into_iter().take(8) {
        let Ok(info) = Command::new("/usr/bin/lsappinfo").args(["info", &asn]).output() else { continue };
        if let Some(f) = parse_lsappinfo(&String::from_utf8_lossy(&info.stdout)).filter(counts_as_target) {
            return Some(f);
        }
    }
    None
}

/// Start the watcher. `panel_open` tells it to also report key presses.
pub fn start(tx: Sender<HotkeyMsg>, ctx: eframe::egui::Context, panel_open: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    // The last app in front that counts, kept for when Keyclash itself (or Spotlight…) is in front.
    // The app in front is also read at the moment of ⌘ ⌘ (`target_app`), so this slow check is only the fallback.
    std::thread::spawn(move || loop {
        if let Some(f) = front_app() {
            if counts_as_target(&f) {
                *last_other().lock().unwrap() = Some(f);
            }
        }
        std::thread::sleep(FRONT_APP_CHECK);
    });
    DEBUG_KEYS.store(
        crate::sources::data_dir().join("debug-keys").exists(),
        std::sync::atomic::Ordering::Relaxed,
    );
    crate::usage::start();
    let (otx, orx) = std::sync::mpsc::channel::<Obs>();
    start_tap(otx);
    std::thread::spawn(move || {
        let mut det = Detector::default();
        let trigger_file = crate::sources::data_dir().join("open-panel");
        let mut keydown_mark = seconds_since_key_down();
        // `pressed`: the keys were pressed (not the test hook), so the use counter gets ⌘ ⌘.
        let send = |a: Action, pressed: bool| {
            // Letting go of the keys needs no app; opening the panel reads the app in front right now.
            let front = if a == Action::ReleasedAll { None } else { target_app() };
            if let (true, Some(mods)) = (pressed && a != Action::ReleasedAll, crate::settings::get().panel_key.mods()) {
                crate::usage::trigger_pressed(crate::model::Trigger::ModTap { mods, taps: 2 }, front.as_ref().map(|f| f.bundle.clone()));
            }
            match a {
                Action::Toggle => {
                    log(&format!("double press: toggle panel for {:?}", front.as_ref().map(|f| f.bundle.clone())));
                    let _ = tx.send(HotkeyMsg::Toggle(front));
                }
                Action::ShowHeld => {
                    log(&format!("double press held: show panel for {:?}", front.as_ref().map(|f| f.bundle.clone())));
                    let _ = tx.send(HotkeyMsg::ShowHeld(front));
                }
                Action::ReleasedAll => {
                    let _ = tx.send(HotkeyMsg::ReleasedAll);
                }
            }
            ctx.request_repaint();
        };
        let mut hook_checked = Instant::now();
        let mut seen_events = (0u64, 0u64);
        loop {
            // The panel key from the settings (the person can change it while the app runs).
            det.set_key(crate::settings::panel_flag());
            // Events from the tap, or a poll when the tap is not running.
            let mut key_pressed = false;
            let mut pressed_keys: Option<crate::model::Trigger> = None;
            // Modifier changes and clicks, in the order they came: a click ends a double press.
            let mut steps: Vec<Option<i64>> = Vec::new();
            if TAP_ACTIVE.load(std::sync::atomic::Ordering::Relaxed) {
                // The tap wakes this thread on every key event. Without one, the only timed work is
                // the hold delay after a second press; otherwise sleep longer.
                let wait = if det.waiting_for_hold() { POLL } else { IDLE_WAIT };
                if let Ok(o) = orx.recv_timeout(wait) {
                    let mut push = |o: Obs| match o {
                        Obs::Flags(f) => steps.push(Some(f)),
                        Obs::Click => steps.push(None),
                        Obs::KeyDown(t) => {
                            key_pressed = true;
                            pressed_keys = t;
                        }
                    };
                    push(o);
                    while let Ok(o) = orx.try_recv() {
                        push(o);
                    }
                }
            } else {
                std::thread::sleep(POLL);
                while orx.try_recv().is_ok() {}
                let f = modifier_flags();
                if f != det.flags {
                    steps.push(Some(f));
                }
                let since_key = seconds_since_key_down();
                key_pressed = since_key < keydown_mark - 0.001 || since_key < POLL.as_secs_f64();
                keydown_mark = since_key;
            }
            if key_pressed {
                det.key_pressed();
                if panel_open.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = tx.send(HotkeyMsg::KeyPressed(pressed_keys.take()));
                    ctx.request_repaint();
                }
            }
            for step in steps {
                let Some(f) = step else {
                    det.key_pressed();
                    continue;
                };
                if DEBUG_KEYS.load(std::sync::atomic::Ordering::Relaxed) {
                    log(&format!("flags {f:#x}"));
                }
                let now = Instant::now();
                for a in det.flags(f, now) {
                    send(a, true);
                }
            }
            if let Some(a) = det.tick(Instant::now()) {
                send(a, true);
            }
            // Test hook: the file `open-panel` next to the log acts like ⌘ ⌘ (it is removed at once).
            if hook_checked.elapsed() >= IDLE_WAIT {
                hook_checked = Instant::now();
                if DEBUG_KEYS.load(std::sync::atomic::Ordering::Relaxed) {
                    let now = (KEY_EVENTS.load(std::sync::atomic::Ordering::Relaxed), FLAG_EVENTS.load(std::sync::atomic::Ordering::Relaxed));
                    if now != seen_events {
                        seen_events = now;
                        log(&format!("tap events so far: {} key presses, {} modifier changes; counter on {}", now.0, now.1, crate::usage::counting() && !crate::usage::paused()));
                    }
                }
                if trigger_file.exists() && std::fs::remove_file(&trigger_file).is_ok() {
                    send(Action::Toggle, false);
                }
            }
        }
    });
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Action {
    Toggle,
    ShowHeld,
    ReleasedAll,
}

/// The double-press rule on its own, so it can be tested without a keyboard.
pub struct Detector {
    pub flags: i64,
    /// The NSEvent flag of the modifier key that opens the panel. 0: no key opens it.
    pub key: i64,
    was_cmd_only: bool,
    was_any: bool,
    first_press: Option<Instant>,
    second_press: Option<Instant>,
    held_open: bool,
}

impl Default for Detector {
    fn default() -> Self {
        Detector { flags: 0, key: NS_COMMAND, was_cmd_only: false, was_any: false, first_press: None, second_press: None, held_open: false }
    }
}

impl Detector {
    /// Use another panel key. A press sequence that is in progress stops.
    pub fn set_key(&mut self, key: i64) {
        if key != self.key {
            self.key = key;
            self.first_press = None;
            self.second_press = None;
            self.was_cmd_only = false;
        }
    }

    /// True between the second press and the hold delay: `tick` must run soon.
    pub fn waiting_for_hold(&self) -> bool {
        self.second_press.is_some()
    }

    /// A non-modifier key: not a clean double press.
    pub fn key_pressed(&mut self) {
        self.first_press = None;
        self.second_press = None;
    }

    /// The modifier state changed to `f` at `now`.
    pub fn flags(&mut self, f: i64, now: Instant) -> Vec<Action> {
        let mut out = Vec::new();
        self.flags = f;
        let cmd_only = self.key != 0 && f == self.key;
        let any = f != 0;
        // The panel key pressed down by itself (edge).
        if cmd_only && !self.was_cmd_only && !self.was_any {
            match self.first_press {
                Some(t) if now.duration_since(t).as_secs_f64() <= KEY_BETWEEN_DELAY => {
                    self.first_press = None;
                    self.second_press = Some(now);
                }
                _ => self.first_press = Some(now),
            }
        }
        // Second press let go early: open and stay.
        if let Some(t) = self.second_press {
            if !cmd_only {
                self.second_press = None;
                if !any && now.duration_since(t).as_secs_f64() < KEY_HOLDING_DELAY {
                    out.push(Action::Toggle);
                }
            }
        }
        if self.held_open && !any {
            self.held_open = false;
            out.push(Action::ReleasedAll);
        }
        if any && !cmd_only {
            self.first_press = None;
        }
        self.was_cmd_only = cmd_only;
        self.was_any = any;
        out
    }

    /// Time passes: second press held long enough shows the panel while held.
    pub fn tick(&mut self, now: Instant) -> Option<Action> {
        let t = self.second_press?;
        if self.key != 0 && self.flags == self.key && now.duration_since(t).as_secs_f64() >= KEY_HOLDING_DELAY {
            self.second_press = None;
            self.held_open = true;
            return Some(Action::ShowHeld);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_peek_opened_from_keyclash_keeps_keyclash_active() {
        let origin = FrontApp { bundle: crate::bundle::BUNDLE_ID.into(), pid: 1234, name: "Keyclash".into(), path: None };
        assert!(!should_return_focus(&origin), "Closing Peek must not switch away from the Keyclash window that opened it");
    }

    #[test]
    fn closing_peek_can_return_to_the_external_app_that_opened_it() {
        let origin = FrontApp { bundle: "com.apple.Safari".into(), pid: std::process::id() as i32 + 1, name: "Safari".into(), path: None };
        assert!(should_return_focus(&origin));
    }

    #[test]
    fn reads_lsappinfo() {
        let t = "\"Safari\" ASN:0x0-0x38e58e2: (in front) \n    bundleID=\"com.apple.Safari\"\n    bundle path=\"/Applications/Safari.app\"\n    pid = 69062 type=\"Foreground\"\n";
        let f = parse_lsappinfo(t).unwrap();
        assert_eq!((f.bundle.as_str(), f.pid, f.name.as_str()), ("com.apple.Safari", 69062, "Safari"));
    }
    #[test]
    fn reads_visible_process_list() {
        let t = "ASN:0x0-0x19019-\"Google_Chrome\": ASN:0x0-0x3f12f0f-\"Shortcut_Atlas\": ASN:0x0-0x2589587-\"T3_Code_(Nightly)\": ";
        assert_eq!(visible_asns(t), vec!["ASN:0x0-0x19019:", "ASN:0x0-0x3f12f0f:", "ASN:0x0-0x2589587:"]);
    }

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    #[test]
    fn quick_double_tap_toggles() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        assert!(d.flags(NS_COMMAND, at(t0, 0)).is_empty());
        assert!(d.flags(0, at(t0, 80)).is_empty());
        assert!(d.flags(NS_COMMAND, at(t0, 200)).is_empty());
        assert_eq!(d.flags(0, at(t0, 280)), vec![Action::Toggle]);
    }

    #[test]
    fn a_click_inside_a_double_press_is_not_peek() {
        // Two ⌘-clicks: press, click, let go, two times.
        let t0 = Instant::now();
        let mut d = Detector::default();
        let mut out = Vec::new();
        out.extend(d.flags(NS_COMMAND, at(t0, 0)));
        d.key_pressed();
        out.extend(d.flags(0, at(t0, 80)));
        out.extend(d.flags(NS_COMMAND, at(t0, 200)));
        d.key_pressed();
        out.extend(d.flags(0, at(t0, 280)));
        assert!(out.is_empty(), "{out:?}");
        // A click only in the second press also stops it.
        let mut d = Detector::default();
        d.flags(NS_COMMAND, at(t0, 1000));
        d.flags(0, at(t0, 1080));
        d.flags(NS_COMMAND, at(t0, 1200));
        d.key_pressed();
        assert!(d.flags(0, at(t0, 1280)).is_empty());
    }

    #[test]
    fn double_press_and_hold_shows_then_hides() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        d.flags(NS_COMMAND, at(t0, 0));
        d.flags(0, at(t0, 80));
        d.flags(NS_COMMAND, at(t0, 200));
        assert_eq!(d.tick(at(t0, 400)), None);
        assert_eq!(d.tick(at(t0, 720)), Some(Action::ShowHeld));
        assert_eq!(d.flags(0, at(t0, 900)), vec![Action::ReleasedAll]);
    }

    #[test]
    fn the_panel_key_from_the_settings_is_used() {
        let t0 = Instant::now();
        // Option is the panel key: Option twice opens the panel, Command twice does not.
        let mut d = Detector::default();
        d.set_key(NS_OPTION);
        d.flags(NS_OPTION, at(t0, 0));
        d.flags(0, at(t0, 80));
        d.flags(NS_OPTION, at(t0, 200));
        assert_eq!(d.flags(0, at(t0, 280)), vec![Action::Toggle]);
        d.flags(NS_COMMAND, at(t0, 1000));
        d.flags(0, at(t0, 1080));
        d.flags(NS_COMMAND, at(t0, 1200));
        assert!(d.flags(0, at(t0, 1280)).is_empty());
        // The panel key is off: no key opens the panel, also when it is held.
        let mut d = Detector::default();
        d.set_key(0);
        d.flags(NS_COMMAND, at(t0, 0));
        d.flags(0, at(t0, 80));
        d.flags(NS_COMMAND, at(t0, 200));
        assert_eq!(d.tick(at(t0, 900)), None);
        assert!(d.flags(0, at(t0, 950)).is_empty());
    }

    #[test]
    fn slow_presses_or_other_keys_do_nothing() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        d.flags(NS_COMMAND, at(t0, 0));
        d.flags(0, at(t0, 80));
        d.flags(NS_COMMAND, at(t0, 700));
        assert!(d.flags(0, at(t0, 760)).is_empty());
        // ⌘C typed twice is not a double press.
        let mut d = Detector::default();
        d.flags(NS_COMMAND, at(t0, 0));
        d.key_pressed();
        d.flags(0, at(t0, 80));
        d.flags(NS_COMMAND, at(t0, 200));
        assert!(d.flags(0, at(t0, 260)).is_empty());
    }
}
