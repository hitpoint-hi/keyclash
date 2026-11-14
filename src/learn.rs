//! Learn menus in the background. Many apps build their menus in code, so their shortcuts
//! can only be read from the live menu bar. This watcher sees an app start, reads its menu
//! bar a few seconds later and remembers the result, so the list fills while you work.

use crate::model::*;
use crate::sources;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub enum LearnMsg {
    /// Bundle ids of the apps that are open now. Sent when the set changes.
    Running(HashSet<String>),
    /// Menus read from an app that started a moment ago.
    Learned { name: String, bundle: String, shortcuts: Vec<Shortcut> },
}

/// How often the list of running apps is read.
const POLL: Duration = Duration::from_secs(3);

/// Seconds after an app is first seen at which its menu bar is read.
/// A slow app builds its menus late, so a later read can find more.
const READ_AT: [u64; 3] = [4, 12, 30];

/// True when read number `tries` (from 0) of an app first seen `elapsed` seconds ago is due.
fn due(elapsed: u64, tries: usize) -> bool {
    READ_AT.get(tries).is_some_and(|&at| elapsed >= at)
}

/// An app that started while the watcher runs.
struct Fresh {
    bundle: String,
    name: String,
    seen: Instant,
    tries: usize,
    /// The largest number of shortcuts a read returned.
    best: usize,
}

// The process list, straight from the system. The watcher runs for as long as the app does,
// so it must not start a `ps` program every few seconds (about 50 ms of CPU each time).
unsafe extern "C" {
    /// Fills `buffer` with process ids; returns how many there are.
    fn proc_listallpids(buffer: *mut std::ffi::c_void, buffersize: i32) -> i32;
    /// Writes the program path of `pid` into `buffer`; returns its length, or 0.
    fn proc_pidpath(pid: i32, buffer: *mut std::ffi::c_void, buffersize: u32) -> i32;
}

/// The ids of all processes: one system call.
fn all_pids() -> Vec<i32> {
    let n = unsafe { proc_listallpids(std::ptr::null_mut(), 0) };
    if n <= 0 {
        return Vec::new();
    }
    // Room for processes that start between the two calls.
    let mut buf = vec![0i32; n as usize + 64];
    let got = unsafe { proc_listallpids(buf.as_mut_ptr().cast(), (buf.len() * 4) as i32) };
    buf.truncate(got.clamp(0, buf.len() as i32) as usize);
    buf
}

/// The program file of a process. None for a process of another user.
fn program_of(pid: i32) -> Option<String> {
    // PROC_PIDPATHINFO_MAXSIZE is 4 * MAXPATHLEN.
    let mut buf = [0u8; 4096];
    let n = unsafe { proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    (n > 0).then(|| String::from_utf8_lossy(&buf[..n as usize]).to_string())
}

/// The installed app at a bundle path.
fn app_at<'a>(apps: &'a [AppInfo], path: &str) -> Option<&'a AppInfo> {
    let canon = std::fs::canonicalize(path).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| path.to_string());
    apps.iter().find(|a| a.path == canon)
}

/// True for an app whose menus are worth a read.
fn readable(app: &AppInfo) -> bool {
    !app.background_only && app.bundle.as_deref().is_some_and(|b| !sources::IGNORED.contains(&b))
}

/// True when an app at this path can be missing from the list only because it was installed a moment ago.
fn in_app_folder(path: &str) -> bool {
    path.starts_with("/Applications/") || path.starts_with(&format!("{}/Applications/", sources::home().display()))
}

/// True when a process that is not an app will stay that way, so it needs no more lookups.
/// macOS starts an app as the launcher `xpcproxy`, which then turns into the app with the same
/// process id. A lookup in that moment sees the launcher, so a new process gets a second look
/// on the next pass, and a launcher gets a few more.
fn settled(program: Option<&str>, looks: u8) -> bool {
    let launcher = program.is_some_and(|p| p.ends_with("/xpcproxy"));
    looks >= if launcher { 5 } else { 2 }
}

/// The list of installed apps, read again at most this often.
const APP_LIST_MAX_AGE: Duration = Duration::from_secs(30);

/// Start the watcher thread. It runs until the app quits.
///
/// One pass costs one system call when no process started. Only a process that is new since
/// the last pass gets a path lookup, so the work follows what changed, not what runs.
pub fn start(tx: mpsc::Sender<LearnMsg>, ctx: eframe::egui::Context) {
    std::thread::spawn(move || {
        let mut apps = sources::installed_apps();
        let mut apps_read = Instant::now();
        // Every process seen so far: the bundle id when it is an installed app, else None.
        let mut seen: HashMap<i32, Option<String>> = HashMap::new();
        // New processes that are not an app yet, and how often each was looked at.
        let mut unsettled: HashMap<i32, u8> = HashMap::new();
        let mut fresh: HashMap<i32, Fresh> = HashMap::new();
        let mut last_running: Option<HashSet<String>> = None;
        // Apps that are open at the start belong to the scan, not to the watcher.
        let mut first_pass = true;
        loop {
            if !first_pass {
                std::thread::sleep(POLL);
            }
            let pids = all_pids();
            let mut set_changed = first_pass;
            for &pid in &pids {
                if seen.contains_key(&pid) {
                    continue;
                }
                let program = program_of(pid);
                let bundle_path = program.as_deref().and_then(sources::app_of_program);
                let mut app = bundle_path.and_then(|p| app_at(&apps, p));
                if let (None, Some(p)) = (app, bundle_path) {
                    // An app installed after the start is not in the list yet: read the list again.
                    if !first_pass && in_app_folder(p) && apps_read.elapsed() > APP_LIST_MAX_AGE {
                        apps = sources::installed_apps();
                        apps_read = Instant::now();
                        app = app_at(&apps, p);
                    }
                }
                let bundle = app.and_then(|a| a.bundle.clone());
                if let (Some(a), Some(b), false) = (app, &bundle, first_pass) {
                    if readable(a) {
                        crate::hotkey::log(&format!("learn: {} started", a.name));
                        fresh.insert(pid, Fresh { bundle: b.clone(), name: a.name.clone(), seen: Instant::now(), tries: 0, best: 0 });
                    }
                }
                if bundle.is_none() && !first_pass {
                    let looks = unsettled.entry(pid).or_insert(0);
                    *looks += 1;
                    if !settled(program.as_deref(), *looks) {
                        continue;
                    }
                }
                unsettled.remove(&pid);
                set_changed |= bundle.is_some();
                seen.insert(pid, bundle);
            }
            first_pass = false;
            let alive: HashSet<i32> = pids.iter().copied().collect();
            seen.retain(|pid, bundle| {
                let keep = alive.contains(pid);
                set_changed |= !keep && bundle.is_some();
                keep
            });
            unsettled.retain(|pid, _| alive.contains(pid));
            fresh.retain(|pid, f| alive.contains(pid) && f.tries < READ_AT.len());
            let mut changed = false;
            if set_changed {
                let running: HashSet<String> = seen.values().flatten().cloned().collect();
                if last_running.as_ref() != Some(&running) {
                    last_running = Some(running.clone());
                    changed = tx.send(LearnMsg::Running(running)).is_ok();
                }
            }
            for (pid, f) in fresh.iter_mut() {
                if !due(f.seen.elapsed().as_secs(), f.tries) {
                    continue;
                }
                f.tries += 1;
                if !crate::ax::is_trusted() {
                    continue;
                }
                let limit = sources::BROWSERS.contains(&f.bundle.as_str()).then_some(sources::BROWSER_MENU_LIMIT);
                let Some(items) = crate::ax::read_app_menus(*pid, sources::MAX_LIVE_ITEMS, limit) else { continue };
                let list = sources::live_items_to_shortcuts(&f.name, &f.bundle, items);
                // Keep the largest read: an early read can catch a menu bar that is not complete.
                if list.len() <= f.best {
                    continue;
                }
                f.best = list.len();
                crate::hotkey::log(&format!("learn: read {} shortcuts from {} (read {})", list.len(), f.name, f.tries));
                sources::remember_menus(&f.bundle, list.clone());
                changed |= tx.send(LearnMsg::Learned { name: f.name.clone(), bundle: f.bundle.clone(), shortcuts: list }).is_ok();
            }
            if changed {
                ctx.request_repaint();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_are_due_in_order() {
        assert!(!due(3, 0));
        assert!(due(4, 0));
        // The second read waits for its own time, also when the first one was late.
        assert!(!due(11, 1));
        assert!(due(12, 1));
        assert!(due(30, 2));
        // No fourth read.
        assert!(!due(600, 3));
    }

    #[test]
    fn the_process_list_has_this_process_and_its_program() {
        let me = std::process::id() as i32;
        assert!(all_pids().contains(&me));
        let exe = std::env::current_exe().unwrap();
        let program = program_of(me).unwrap();
        assert_eq!(std::fs::canonicalize(program).unwrap(), std::fs::canonicalize(exe).unwrap());
    }

    #[test]
    fn a_new_process_gets_a_second_look_and_a_launcher_more() {
        assert!(!settled(Some("/usr/bin/git"), 1));
        assert!(settled(Some("/usr/bin/git"), 2));
        // The launcher turns into the app a moment later.
        assert!(!settled(Some("/usr/libexec/xpcproxy"), 2));
        assert!(settled(Some("/usr/libexec/xpcproxy"), 5));
        // A process of another user has no readable path.
        assert!(settled(None, 2));
    }

    #[test]
    fn a_program_maps_to_its_app() {
        use sources::app_of_program as f;
        assert_eq!(f("/Applications/Safari.app/Contents/MacOS/Safari"), Some("/Applications/Safari.app"));
        assert_eq!(f("/Applications/A.app/Contents/Frameworks/B.app/Contents/MacOS/B"), None);
        assert_eq!(f("/usr/sbin/cfprefsd"), None);
    }

    /// One pass of the watcher with no new process must stay far below one millisecond.
    #[test]
    fn a_quiet_pass_is_cheap() {
        let t = Instant::now();
        for _ in 0..100 {
            let _: HashSet<i32> = all_pids().into_iter().collect();
        }
        let per_pass = t.elapsed() / 100;
        assert!(per_pass < Duration::from_millis(1), "one pass took {per_pass:?}");
        eprintln!("one quiet pass: {per_pass:?}");
    }

    #[test]
    fn background_and_ignored_apps_are_not_read() {
        let app = |bundle: Option<&str>, bg: bool| AppInfo { name: "X".into(), bundle: bundle.map(String::from), background_only: bg, ..Default::default() };
        assert!(readable(&app(Some("com.example.app"), false)));
        assert!(!readable(&app(Some("com.example.app"), true)));
        assert!(!readable(&app(None, false)));
        assert!(!readable(&app(Some(sources::IGNORED[0]), false)));
    }
}
