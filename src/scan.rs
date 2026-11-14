//! Runs every source and merges the results.

use crate::model::*;
use crate::sources::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Default)]
pub struct ScanResult {
    pub shortcuts: Vec<Shortcut>,
    pub apps: Vec<AppInfo>,
    pub notes: Vec<SourceNote>,
    /// Bundle ids whose menus were read live in this scan.
    pub live_bundles: Vec<String>,
    /// Bundle ids with remembered menus, and when (seconds since 1970).
    pub remembered: Vec<(String, u64)>,
    pub ax_trusted: bool,
}

/// Fast sources: files on disk only. Runs before the first frame.
pub fn quick_scan() -> ScanResult {
    let t0 = std::time::Instant::now();
    let lap = |what: &str| if std::env::var("SA_TIME").is_ok() { eprintln!("{what}: {:?}", t0.elapsed()) };
    let mut apps = installed_apps();
    lap("apps");
    let running: HashSet<String> = running_apps()
        .into_iter()
        .map(|(_, p)| std::fs::canonicalize(&p).map(|c| c.to_string_lossy().to_string()).unwrap_or(p))
        .collect();
    for a in apps.iter_mut() {
        a.running = running.contains(&a.path);
    }
    let mut r = ScanResult { ax_trusted: crate::ax::is_trusted(), ..Default::default() };

    lap("running");
    let apps_ref = &apps;
    let ((sys, sys_note), (set, set_note), cfg) = std::thread::scope(|sc| {
        let h_sys = sc.spawn(|| {
            let (mut s, n) = system_shortcuts();
            s.extend(fn_key_usage());
            (s, n)
        });
        let h_set = sc.spawn(|| settings_shortcuts(apps_ref));
        let h_cfg = sc.spawn(|| {
            let mut cfg = Vec::new();
            cfg.extend(aerospace());
            cfg.extend(omniwm());
            cfg.extend(skhd(&[]));
            cfg.extend(karabiner());
            cfg.extend(wispr_flow());
            cfg.extend(editor_keybindings(apps_ref));
            cfg.extend(builtin_app_hotkeys(apps_ref));
            cfg
        });
        (h_sys.join().unwrap_or_default(), h_set.join().unwrap_or_default(), h_cfg.join().unwrap_or_default())
    });
    lap("settings + configs");
    r.shortcuts.extend(sys);
    r.notes.push(sys_note);
    let rect = rectangle_defaults(&apps, &set);
    r.shortcuts.extend(set);
    r.notes.push(set_note);
    let rn = rect.len();
    r.shortcuts.extend(rect);
    if rn > 0 {
        r.notes.push(SourceNote { name: "Rectangle built-in defaults".into(), count: rn, status: "not stored in settings; taken from Rectangle's defaults (guess)".into(), ok: true });
    }
    let mut cfg = cfg;
    for s in cfg.iter_mut() {
        // Use the installed app's own name and id when we can match it.
        if let Some(a) = apps.iter().find(|a| a.name.eq_ignore_ascii_case(&s.app) || a.name.starts_with(&s.app)) {
            s.bundle = a.bundle.clone();
            s.app = a.name.clone();
        }
    }
    let nc = cfg.len();
    r.shortcuts.extend(cfg);
    r.notes.push(SourceNote { name: "Tool config files".into(), count: nc, status: "AeroSpace, OmniWM, skhd, Karabiner, Wispr Flow, editor key bindings".into(), ok: true });

    let js = json_settings_hotkeys(&apps);
    if !js.is_empty() {
        let owners: HashSet<&str> = js.iter().map(|s| s.app.as_str()).collect();
        let mut names: Vec<&str> = owners.into_iter().collect();
        names.sort();
        r.notes.push(SourceNote { name: "App JSON settings".into(), count: js.len(), status: format!("hotkeys stored as text: {}", names.join(", ")), ok: true });
        r.shortcuts.extend(js);
    }

    let (menus, n) = menu_file_shortcuts(&mut apps);
    lap("menus");
    r.shortcuts.extend(menus);
    r.notes.push(n);

    // Menus read live in earlier runs.
    let cache = load_cache();
    let mut remembered = 0;
    for app in &apps {
        let Some((when, list)) = cache.menus_for(app) else { continue };
        let Some(b) = &app.bundle else { continue };
        r.remembered.push((b.clone(), when));
        for s in list {
            let mut s = s.clone();
            s.origin = Origin::RememberedMenu;
            r.shortcuts.push(s);
            remembered += 1;
        }
    }
    if remembered > 0 {
        r.notes.push(SourceNote { name: "Remembered live menus".into(), count: remembered, status: format!("{} installed app versions read on an earlier scan", r.remembered.len()), ok: true });
    }
    r.apps = apps;
    r
}

/// Slow sources: Electron bundles, sandboxed settings and live menus. Runs in the background.
pub fn deep_scan(base: &ScanResult) -> ScanResult {
    let mut r = ScanResult { apps: base.apps.clone(), ax_trusted: crate::ax::is_trusted(), ..Default::default() };
    let (el, n) = electron_shortcuts(&r.apps);
    r.shortcuts.extend(el);
    r.notes.push(n);

    let already: HashSet<String> = std::fs::read_dir(home().join("Library/Preferences"))
        .map(|rd| rd.flatten().filter_map(|e| e.path().file_stem().map(|s| s.to_string_lossy().to_string())).collect())
        .unwrap_or_default();
    let (cs, n) = container_settings_shortcuts(&r.apps, &already);
    r.shortcuts.extend(cs);
    r.notes.push(n);

    let (live, bundles, n) = live_menu_shortcuts(&r.apps);
    r.notes.push(n);
    if !bundles.is_empty() {
        let now = now_secs();
        update_menu_cache(|cache| {
            for b in &bundles {
                let list: Vec<Shortcut> = live.iter().filter(|s| s.bundle.as_deref() == Some(b)).cloned().collect();
                if let Some(app) = r.apps.iter().find(|a| a.bundle.as_ref() == Some(b)) {
                    cache.remember(app, now, list);
                }
            }
        });
    }
    r.live_bundles = bundles;
    r.shortcuts.extend(live);
    r
}

/// Keep menus that were read live a moment ago as the remembered menus of a files-only result.
pub fn remember_in(quick: &mut ScanResult, bundle: &str, list: &[Shortcut], now: u64) {
    quick.shortcuts.retain(|s| !(s.origin == Origin::RememberedMenu && s.bundle.as_deref() == Some(bundle)));
    quick.shortcuts.extend(list.iter().cloned().map(|mut s| {
        s.origin = Origin::RememberedMenu;
        s
    }));
    quick.remembered.retain(|(b, _)| b != bundle);
    quick.remembered.push((bundle.to_string(), now));
}

/// Put menus that were read live a moment ago into a deep result, in place of an older live read.
pub fn add_live(deep: &mut ScanResult, bundle: &str, list: &[Shortcut]) {
    deep.shortcuts.retain(|s| !(s.origin == Origin::LiveMenu && s.bundle.as_deref() == Some(bundle)));
    deep.shortcuts.extend(list.iter().cloned());
    if !deep.live_bundles.iter().any(|b| b == bundle) {
        deep.live_bundles.push(bundle.to_string());
    }
}

/// Combine quick and deep results. Live menus replace remembered ones, and a menu
/// read from the running app replaces what was read from its files.
pub fn merge(quick: &ScanResult, deep: Option<&ScanResult>) -> ScanResult {
    let mut r = quick.clone();
    if let Some(d) = deep {
        r.shortcuts.extend(d.shortcuts.iter().cloned());
        r.notes.extend(d.notes.iter().cloned());
        r.live_bundles = d.live_bundles.clone();
        r.ax_trusted = d.ax_trusted;
    }
    let live: HashSet<String> = r.live_bundles.iter().cloned().collect();
    let remembered: HashSet<String> = r.remembered.iter().map(|(b, _)| b.clone()).collect();
    r.shortcuts.retain(|s| {
        let b = s.bundle.clone().unwrap_or_default();
        match s.origin {
            Origin::RememberedMenu => !live.contains(&b),
            Origin::MenuFile | Origin::ElectronBundle if s.scope == Scope::App => !live.contains(&b) && !remembered.contains(&b),
            _ => true,
        }
    });
    // One name per app: every source uses the name of the installed app with the same id.
    // A remembered or live menu can carry the name the app gives itself, which two apps can share.
    let names: HashMap<&str, &str> = r.apps.iter().filter_map(|a| Some((a.bundle.as_deref()?, a.name.as_str()))).collect();
    for s in &mut r.shortcuts {
        if let Some(name) = s.bundle.as_deref().and_then(|b| names.get(b)) {
            if s.app != *name {
                s.app = name.to_string();
            }
        }
    }
    r
}

#[cfg(test)]
mod learn_tests {
    use super::*;

    fn item(action: &str, origin: Origin) -> Shortcut {
        Shortcut::new("Word", action, Trigger::combo(CMD, "K"), Scope::App, origin).bundle(Some("com.microsoft.Word"))
    }

    fn actions(r: &ScanResult) -> Vec<(String, Origin)> {
        r.shortcuts.iter().map(|s| (s.action.clone(), s.origin)).collect()
    }

    #[test]
    fn a_learned_menu_replaces_the_menu_file_before_the_deep_scan_ends() {
        let mut quick = ScanResult { shortcuts: vec![item("From file", Origin::MenuFile), item("Old memory", Origin::RememberedMenu)], ..Default::default() };
        remember_in(&mut quick, "com.microsoft.Word", &[item("Insert Link", Origin::LiveMenu)], 7);
        let merged = merge(&quick, None);
        assert_eq!(actions(&merged), [("Insert Link".to_string(), Origin::RememberedMenu)]);
        assert_eq!(quick.remembered, [("com.microsoft.Word".to_string(), 7)]);
    }

    #[test]
    fn a_learned_menu_replaces_an_older_live_read() {
        let mut quick = ScanResult { shortcuts: vec![item("From file", Origin::MenuFile)], ..Default::default() };
        let mut deep = ScanResult { shortcuts: vec![item("Early read", Origin::LiveMenu)], live_bundles: vec!["com.microsoft.Word".into()], ..Default::default() };
        let new = [item("Insert Link", Origin::LiveMenu), item("Find", Origin::LiveMenu)];
        remember_in(&mut quick, "com.microsoft.Word", &new, 7);
        add_live(&mut deep, "com.microsoft.Word", &new);
        let merged = merge(&quick, Some(&deep));
        assert_eq!(actions(&merged), [("Insert Link".to_string(), Origin::LiveMenu), ("Find".to_string(), Origin::LiveMenu)]);
        assert_eq!(deep.live_bundles.len(), 1);
    }
}
