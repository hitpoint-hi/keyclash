//! Use counter: how many times each key combination is pressed, per app and per day.
//!
//! The key listener (`hotkey.rs`) reports each key press that has ⌘, ⌃, ⌥ or 🌐 held.
//! Plain typing never leaves the listener. The file holds counts only: no text, no key order.
//! A press is matched to a shortcut when the list is shown, so old counts follow a better scan.

use crate::keys::*;
use crate::model::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Bundle id used when the app in front is not known.
pub const UNKNOWN_APP: &str = "unknown";
/// How long changed counts stay in memory before they go to the file.
const SAVE_EVERY: Duration = Duration::from_secs(20);

/// day ("2026-11-09") › app (bundle id) › keys (`code`) › presses.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Store {
    pub version: u32,
    /// When counting started (seconds since 1970).
    pub since: u64,
    #[serde(default)]
    pub days: BTreeMap<String, BTreeMap<String, BTreeMap<String, u32>>>,
}

/// One line of the count: these keys, with this app in front, this many times.
pub type Row = (Trigger, String, u32);

impl Store {
    pub fn add(&mut self, day: &str, bundle: &str, t: &Trigger) {
        *self.days.entry(day.to_string()).or_default().entry(bundle.to_string()).or_default().entry(code(t)).or_default() += 1;
    }

    /// Presses from `from_day` on (None: all days), summed per keys and app.
    pub fn rows(&self, from_day: Option<&str>) -> Vec<Row> {
        let mut sum: HashMap<(Trigger, String), u32> = HashMap::new();
        for (_, apps) in self.days.iter().filter(|(d, _)| from_day.is_none_or(|f| d.as_str() >= f)) {
            for (bundle, keys) in apps {
                for (c, n) in keys {
                    if let Some(t) = decode(c) {
                        *sum.entry((t, bundle.clone())).or_default() += n;
                    }
                }
            }
        }
        let mut rows: Vec<Row> = sum.into_iter().map(|((t, b), n)| (t, b, n)).collect();
        rows.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)).then_with(|| a.1.cmp(&b.1)));
        rows
    }
}

/// A trigger as text for the file: "c5:T" (⌥⌘T), "t1:2" (⌘ twice), "h16" (hold 🌐).
pub fn code(t: &Trigger) -> String {
    match t {
        Trigger::Combo { mods, key } => format!("c{mods}:{key}"),
        Trigger::ModTap { mods, taps } => format!("t{mods}:{taps}"),
        Trigger::ModHold { mods } => format!("h{mods}"),
    }
}

pub fn decode(s: &str) -> Option<Trigger> {
    let (kind, rest) = (s.get(..1)?, s.get(1..)?);
    match kind {
        "c" => {
            let (m, key) = rest.split_once(':')?;
            (!key.is_empty()).then_some(Trigger::Combo { mods: m.parse().ok()?, key: key.to_string() })
        }
        "t" => {
            let (m, n) = rest.split_once(':')?;
            Some(Trigger::ModTap { mods: m.parse().ok()?, taps: n.parse().ok()? })
        }
        "h" => Some(Trigger::ModHold { mods: rest.parse().ok()? }),
        _ => None,
    }
}

/// True for a key press that is a shortcut and not typing: ⌘, ⌃, ⌥ or 🌐 is held, or the key is F1 to F20.
pub fn countable(t: &Trigger) -> bool {
    match t {
        Trigger::Combo { mods, key } => mods & (CMD | CTRL | OPT | FN) != 0 || is_f_key(key),
        _ => true,
    }
}

fn is_f_key(key: &str) -> bool {
    key.starts_with('F') && key.len() > 1 && key[1..].chars().all(|c| c.is_ascii_digit())
}

/// The trigger for a key press from the listener (key code and NSEvent-style flags).
pub fn trigger_of(key_code: i64, flags: i64) -> Option<Trigger> {
    if keycode_modifier(key_code).is_some() {
        return None;
    }
    let t = clean_trigger(mods_from_nsevent(flags), keycode_key(key_code)?);
    countable(&t).then_some(t)
}

/// Keys that macOS itself gives a meaning in each app (text editing, app switch). They are not
/// app shortcuts, so they are kept out of the "not in the scan" list.
pub fn standard_key(t: &Trigger) -> Option<&'static str> {
    let Trigger::Combo { mods, key } = t else { return None };
    let (m, k) = (*mods, key.as_str());
    let no_shift = m & !SHIFT;
    let arrow = matches!(k, "Left" | "Right" | "Up" | "Down");
    if arrow && (no_shift == CMD || no_shift == OPT) {
        return Some("Move or select in text");
    }
    if matches!(k, "Delete" | "Forward Delete") && (m == CMD || m == OPT || m == FN) {
        return Some("Delete in text");
    }
    if k == "Tab" && no_shift == CMD {
        return Some("Switch app");
    }
    if m == CTRL && matches!(k, "A" | "E" | "K" | "D" | "F" | "B" | "N" | "P" | "H") {
        return Some("Text keys with Control");
    }
    if no_shift == OPT && k.chars().count() == 1 {
        return Some("Types a special character");
    }
    None
}

// ------------------------------------------------------------------ matching presses to shortcuts

#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Presses credited to each shortcut (same order as the list).
    pub per_shortcut: Vec<u32>,
    /// Presses per key combination, for the keyboard map.
    pub by_trigger: HashMap<Trigger, u32>,
    /// Presses that match no shortcut in the list, most pressed first.
    pub unknown: Vec<Row>,
    /// Presses with several possible actions. Counted once, without choosing a winner.
    pub ambiguous: Vec<Row>,
    /// Presses of standard macOS keys that match no shortcut: (name, presses).
    pub standard: Vec<(&'static str, u32)>,
    pub total: u32,
}

/// Match observed keys and the recorded foreground app, without using today's running apps.
/// A key observation does not prove that an action ran. Several possible actions stay ambiguous.
pub fn attribute(list: &[Shortcut], rows: &[Row]) -> Report {
    let mut by_keys: HashMap<&Trigger, Vec<usize>> = HashMap::new();
    for (i, s) in list.iter().enumerate().filter(|(_, s)| s.enabled && s.scope != Scope::Mode) {
        by_keys.entry(&s.trigger).or_default().push(i);
    }
    let mut r = Report { per_shortcut: vec![0; list.len()], ..Default::default() };
    let mut standard: BTreeMap<&'static str, u32> = BTreeMap::new();
    for (t, bundle, n) in rows {
        r.total += n;
        *r.by_trigger.entry(t.clone()).or_default() += n;
        let cands = by_keys.get(t).map(|v| v.as_slice()).unwrap_or(&[]);
        let mut seen = HashSet::new();
        let hits: Vec<usize> = cands.iter().copied().filter(|&i| {
            let s = &list[i];
            (s.scope == Scope::Global || s.bundle.as_deref().is_some_and(|b| b == "*" || b.eq_ignore_ascii_case(bundle)))
                && seen.insert((s.owner(), s.action.trim().to_lowercase()))
        }).collect();
        if hits.is_empty() {
            match standard_key(t) {
                Some(name) => *standard.entry(name).or_default() += n,
                None => r.unknown.push((t.clone(), bundle.clone(), *n)),
            }
        }
        match hits.as_slice() {
            [i] => r.per_shortcut[*i] += n,
            [] => {},
            _ => r.ambiguous.push((t.clone(), bundle.clone(), *n)),
        }
    }
    r.unknown.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    r.ambiguous.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    r.standard = standard.into_iter().collect();
    r.standard.sort_by(|a, b| b.1.cmp(&a.1));
    r
}

// ------------------------------------------------------------------ dates

#[repr(C)]
struct Tm {
    sec: i32,
    min: i32,
    hour: i32,
    mday: i32,
    mon: i32,
    year: i32,
    wday: i32,
    yday: i32,
    isdst: i32,
    gmtoff: i64,
    zone: *const std::ffi::c_char,
}

unsafe extern "C" {
    fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
}

/// The local date of a moment, as "2026-11-09".
pub fn day_key(secs: u64) -> String {
    let mut tm = std::mem::MaybeUninit::<Tm>::zeroed();
    let t = secs as i64;
    let tm = unsafe {
        if localtime_r(&t, tm.as_mut_ptr()).is_null() {
            return "1970-01-01".into();
        }
        tm.assume_init()
    };
    format!("{:04}-{:02}-{:02}", tm.year + 1900, tm.mon + 1, tm.mday)
}

/// "2026-11-09" as "9 Nov 2026".
pub fn day_words(key: &str) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let mut p = key.split('-').filter_map(|x| x.parse::<usize>().ok());
    match (p.next(), p.next(), p.next()) {
        (Some(y), Some(m), Some(d)) if (1..=12).contains(&m) => format!("{d} {} {y}", MONTHS[m - 1]),
        _ => key.to_string(),
    }
}

// ------------------------------------------------------------------ the count on this Mac

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Range {
    Today,
    Week,
    Month,
    All,
}

impl Range {
    /// The first day of the range.
    fn from_day(self, now: u64) -> Option<String> {
        let back = match self {
            Range::Today => 0,
            Range::Week => 6,
            Range::Month => 29,
            Range::All => return None,
        };
        Some(day_key(now.saturating_sub(back * 86_400)))
    }
}

struct State {
    store: Store,
    dirty: bool,
    last_save: Instant,
}

fn path() -> std::path::PathBuf {
    // `SA_USAGE_FILE`: use another file (screenshots and tests of the window).
    match std::env::var("SA_USAGE_FILE") {
        Ok(p) if !p.is_empty() => p.into(),
        _ => crate::sources::data_dir().join("usage.json"),
    }
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| {
        // A damaged file is kept aside (`usage.json.bad-…`), so the first save cannot write over the counts.
        let mut store: Store = crate::sources::read_json(&path()).unwrap_or_default();
        if store.since == 0 {
            store.since = crate::sources::now_secs();
        }
        store.version = 1;
        Mutex::new(State { store, dirty: false, last_save: Instant::now() })
    })
}

/// Goes up by one on each change, so the window knows when to read the counts again.
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// True while this copy of the app counts key presses.
static COUNTING: AtomicBool = AtomicBool::new(false);
static PAUSED: AtomicBool = AtomicBool::new(true);

pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

/// True when this copy of the app has the counter on (the installed app only).
pub fn counting() -> bool {
    COUNTING.load(Ordering::Relaxed)
}

/// True when the person did not turn the counter on (the start value), or turned it off.
pub fn paused() -> bool {
    !crate::settings::get().count_keys
}

pub fn set_paused(p: bool) {
    crate::settings::update(|s| s.count_keys = !p);
    PAUSED.store(p, Ordering::Relaxed);
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// Remove all counts and start again from now.
pub fn clear() {
    if let Ok(mut g) = state().lock() {
        g.store.days.clear();
        g.store.since = crate::sources::now_secs();
        g.dirty = true;
        save(&mut g);
    }
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// (first day counted, rows in the range).
pub fn snapshot(range: Range) -> (String, Vec<Row>) {
    let now = crate::sources::now_secs();
    match state().lock() {
        Ok(g) => (day_key(g.store.since), g.store.rows(range.from_day(now).as_deref())),
        Err(_) => (day_key(now), Vec::new()),
    }
}

fn save(g: &mut State) {
    if !g.dirty {
        return;
    }
    let p = path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    // Write a new file, then swap it in: a stop in the middle keeps the old counts.
    let tmp = p.with_extension("json.tmp");
    if let Ok(b) = serde_json::to_vec(&g.store) {
        if std::fs::write(&tmp, b).is_ok() && std::fs::rename(&tmp, &p).is_ok() {
            g.dirty = false;
            g.last_save = Instant::now();
        }
    }
}

/// Write counts that are still only in memory (at exit).
pub fn flush() {
    if let Ok(mut g) = state().lock() {
        save(&mut g);
    }
}

fn record(t: &Trigger, bundle: &str) {
    if let Ok(mut g) = state().lock() {
        let day = day_key(crate::sources::now_secs());
        g.store.add(&day, bundle, t);
        g.dirty = true;
    }
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

enum Msg {
    /// A key press from the listener: key code, modifier flags, the process the key goes to.
    Key { code: i64, flags: i64, pid: i32 },
    /// A trigger that the listener worked out itself (⌘ twice).
    Trigger(Trigger, Option<String>),
}

static TX: OnceLock<Sender<Msg>> = OnceLock::new();

/// From the key listener, for each key press that is not a repeat. Only a press with ⌘, ⌃, ⌥ or 🌐
/// (arrows and F-keys carry the 🌐 flag) is passed on. Plain typing stops here.
pub fn key_down(code: i64, flags: i64, pid: i32) {
    if flags & (NS_CONTROL | NS_OPTION | NS_COMMAND | NS_FN) == 0 || PAUSED.load(Ordering::Relaxed) {
        return;
    }
    if let Some(tx) = TX.get() {
        let _ = tx.send(Msg::Key { code, flags, pid });
    }
}

/// From the key listener: a modifier-only trigger was pressed (⌘ twice).
pub fn trigger_pressed(t: Trigger, bundle: Option<String>) {
    if PAUSED.load(Ordering::Relaxed) {
        return;
    }
    if let Some(tx) = TX.get() {
        let _ = tx.send(Msg::Trigger(t, bundle));
    }
}

/// The app that owns a process: `lsappinfo info -only bundleid <pid>` prints `bundleID="com.apple.Safari"`.
fn bundle_of_pid(pid: i32) -> Option<String> {
    let out = std::process::Command::new("/usr/bin/lsappinfo").args(["info", "-only", "bundleid", &pid.to_string()]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let b = text.split("bundleID=\"").nth(1)?.split('"').next()?.to_string();
    (!b.is_empty()).then_some(b)
}

/// Start the counter. Only the installed app counts, so a test copy does not count each press twice.
pub fn start() {
    if !(crate::bundle::running_as_app() || std::env::var("SA_COUNT").is_ok()) {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel::<Msg>();
    if TX.set(tx).is_err() {
        return;
    }
    PAUSED.store(paused(), Ordering::Relaxed);
    COUNTING.store(true, Ordering::Relaxed);
    std::thread::spawn(move || {
        // Process ids of apps, looked up once and kept for ten minutes.
        let mut apps: HashMap<i32, (String, Instant)> = HashMap::new();
        let mut app_of = |pid: i32| -> String {
            if pid > 0 {
                if let Some((b, at)) = apps.get(&pid) {
                    if at.elapsed() < Duration::from_secs(600) {
                        return b.clone();
                    }
                }
                if let Some(b) = bundle_of_pid(pid) {
                    apps.insert(pid, (b.clone(), Instant::now()));
                    return b;
                }
            }
            // The key has no target process, or the process is not an app: use the app in front.
            crate::hotkey::front_app().map(|f| f.bundle).unwrap_or_else(|| UNKNOWN_APP.to_string())
        };
        loop {
            match rx.recv_timeout(SAVE_EVERY) {
                Ok(Msg::Key { code, flags, pid }) => {
                    if let Some(t) = trigger_of(code, flags) {
                        record(&t, &app_of(pid));
                    }
                }
                Ok(Msg::Trigger(t, bundle)) => record(&t, bundle.as_deref().unwrap_or(UNKNOWN_APP)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
            if let Ok(mut g) = state().lock() {
                if g.dirty && g.last_save.elapsed() >= SAVE_EVERY {
                    save(&mut g);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sc(app: &str, bundle: &str, action: &str, t: Trigger, scope: Scope) -> Shortcut {
        Shortcut::new(app, action, t, scope, Origin::MenuFile).bundle(Some(bundle))
    }

    #[test]
    fn a_trigger_keeps_its_keys_in_the_file() {
        for t in [Trigger::combo(CMD | OPT, "T"), Trigger::combo(CMD, ":"), Trigger::ModTap { mods: CMD, taps: 2 }, Trigger::ModHold { mods: FN }] {
            assert_eq!(decode(&code(&t)), Some(t));
        }
        assert_eq!(decode("x1"), None);
        assert_eq!(decode("c1:"), None);
    }

    #[test]
    fn only_shortcut_presses_are_counted() {
        // Key code 17 is T. Plain T and ⇧T are typing.
        assert_eq!(trigger_of(17, 0), None);
        assert_eq!(trigger_of(17, NS_SHIFT), None);
        assert_eq!(trigger_of(17, NS_COMMAND | NS_SHIFT), Some(Trigger::combo(CMD | SHIFT, "T")));
        // An arrow carries the 🌐 flag by itself: a plain arrow is not a shortcut, ⌘← is.
        assert_eq!(trigger_of(123, NS_FN), None);
        assert_eq!(trigger_of(123, NS_FN | NS_COMMAND), Some(Trigger::combo(CMD, "Left")));
        // F5 by itself is a shortcut key. A modifier key is not a key press.
        assert_eq!(trigger_of(96, NS_FN), Some(Trigger::combo(0, "F5")));
        assert_eq!(trigger_of(55, NS_COMMAND), None);
    }

    #[test]
    fn counts_add_per_day_and_range() {
        let mut s = Store::default();
        let t = Trigger::combo(CMD, "T");
        s.add("2026-11-01", "com.apple.Safari", &t);
        s.add("2026-11-09", "com.apple.Safari", &t);
        s.add("2026-11-09", "com.apple.Safari", &t);
        s.add("2026-11-09", "com.apple.mail", &Trigger::combo(CMD, "N"));
        assert_eq!(s.rows(None)[0], (t.clone(), "com.apple.Safari".to_string(), 3));
        let week = s.rows(Some("2026-11-03"));
        assert_eq!(week[0], (t, "com.apple.Safari".to_string(), 2));
        assert_eq!(week.len(), 2);
        // The file text reads back to the same counts.
        let back: Store = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.rows(None), s.rows(None));
    }

    #[test]
    fn historical_presses_keep_uncertain_matches_separate() {
        let opt_space = Trigger::combo(OPT, "Space");
        let cmd_t = Trigger::combo(CMD, "T");
        let list = [
            sc("Todoist", "com.todoist.mac.Todoist", "Quick Add", opt_space.clone(), Scope::Global),
            sc("Music", "com.apple.Music", "Genius Shuffle", opt_space.clone(), Scope::App),
            sc("Safari", "com.apple.Safari", "New Tab", cmd_t.clone(), Scope::App),
            sc("OmniWM", "com.barut.OmniWM", "Toggle", Trigger::combo(OPT, "T"), Scope::Global),
            sc("Photos", "com.apple.Photos", "Titles", Trigger::combo(OPT, "T"), Scope::App),
        ];
        let rows = vec![
            (opt_space.clone(), "com.apple.Music".to_string(), 4),
            (cmd_t.clone(), "com.apple.Safari".to_string(), 7),
            (cmd_t.clone(), "com.apple.Notes".to_string(), 2),
            (Trigger::combo(OPT, "T"), "com.apple.Photos".to_string(), 3),
            (Trigger::combo(OPT, "T"), "com.apple.Notes".to_string(), 1),
            (Trigger::combo(OPT, "Left"), "com.apple.Notes".to_string(), 9),
        ];
        let r = attribute(&list, &rows);
        assert_eq!(r.per_shortcut, vec![0, 0, 7, 1, 0]);
        assert_eq!(r.ambiguous, vec![
            (opt_space, "com.apple.Music".to_string(), 4),
            (Trigger::combo(OPT, "T"), "com.apple.Photos".to_string(), 3),
        ]);
        // ⌘T in Notes matches nothing in the list. ⌥← is a standard text key.
        assert_eq!(r.unknown, vec![(cmd_t.clone(), "com.apple.Notes".to_string(), 2)]);
        assert_eq!(r.standard, vec![("Move or select in text", 9)]);
        assert_eq!(r.by_trigger[&cmd_t], 9);
        assert_eq!(r.total, 26);
        assert_eq!(r.per_shortcut.iter().sum::<u32>() + r.ambiguous.iter().map(|r| r.2).sum::<u32>()
            + r.unknown.iter().map(|r| r.2).sum::<u32>() + r.standard.iter().map(|r| r.1).sum::<u32>(), r.total);
    }

    #[test]
    fn two_global_tools_do_not_each_get_the_same_presses() {
        let trigger = Trigger::ModTap { mods: CMD, taps: 2 };
        let list = [
            sc("Keyclash", "app.keyclash.Keyclash", "Panel", trigger.clone(), Scope::Global),
            sc("Other Tool", "com.example.othertool", "Panel", trigger.clone(), Scope::Global),
        ];
        let rows = [(trigger, "com.apple.Safari".to_string(), 11)];
        let r = attribute(&list, &rows);
        assert_eq!(r.total, 11);
        assert_eq!(r.per_shortcut, [0, 0]);
        assert_eq!(r.ambiguous, rows);
    }

    #[test]
    fn duplicate_sources_count_one_match() {
        let trigger = Trigger::combo(CMD, "T");
        let s = sc("Safari", "com.apple.Safari", "New Tab", trigger.clone(), Scope::App);
        let r = attribute(&[s.clone(), s], &[(trigger, "com.apple.Safari".to_string(), 7)]);
        assert_eq!(r.per_shortcut, [7, 0]);
        assert!(r.ambiguous.is_empty());
    }

    #[test]
    fn dates() {
        assert_eq!(day_words("2026-11-09"), "9 Nov 2026");
        // Noon UTC on 9 Nov 2026 is 8, 9 or 10 Nov in each time zone.
        let k = day_key(1_794_225_600);
        assert!(["2026-11-08", "2026-11-09", "2026-11-10"].contains(&k.as_str()), "{k}");
    }
}
