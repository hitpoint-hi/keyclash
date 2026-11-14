//! The window: clashes first, then lookup, the full list, apps and sources.

use crate::conflicts::{self, Conflict, Kind};
use crate::keys::*;
use crate::model::*;
use crate::scan::{self, ScanResult};
use crate::theme::{self, callout, cap, card, chip, dot, medium, num, overline, pill, segmented, semi, tab, th};
use eframe::egui::{self, Color32, RichText};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// How long the header shows what the background watcher learned.
const LEARNED_NOTE_SECS: f32 = 12.0;

const KINDS: [Kind; 5] = [Kind::Clash, Kind::Blocks, Kind::SameModifier, Kind::InsideApp, Kind::Shared];

/// Draw a trigger as key caps.
pub fn keycaps(ui: &mut egui::Ui, t: &Trigger, size: f32) {
    let th = th();
    let response = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let (caps, key) = trigger_caps(t);
        // Parts in reading order; reversed when the row is laid out right to left.
        let mut parts: Vec<(String, Color32, bool)> = caps.iter().map(|c| (c.clone(), th.sym(c), true)).collect();
        match t {
            Trigger::Combo { .. } => parts.push((key, th.text, true)),
            _ => parts.push((key, th.muted, false)),
        }
        if ui.layout().main_dir() == egui::Direction::RightToLeft {
            parts.reverse();
        }
        for (text, color, is_cap) in parts {
            if is_cap {
                cap(ui, &text, color, size);
            } else {
                ui.label(RichText::new(text).color(color).size(size - 1.0).italics());
            }
        }
    });
    response.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Group, ui.is_enabled(), trigger_words(t)));
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Keyboard,
    Conflicts,
    Lookup,
    All,
    Apps,
    Usage,
    Sources,
}

/// The "Works" choice of the All shortcuts tab.
#[derive(PartialEq, Clone, Copy, Debug)]
enum ScopeFilter {
    All,
    Global,
    App,
    Mode,
}

impl ScopeFilter {
    fn keeps(self, s: &Shortcut) -> bool {
        match self {
            ScopeFilter::All => true,
            ScopeFilter::Global => s.scope == Scope::Global,
            ScopeFilter::App => s.scope == Scope::App,
            ScopeFilter::Mode => s.scope == Scope::Mode,
        }
    }
}

/// The column the All shortcuts table is sorted by.
#[derive(PartialEq, Clone, Copy, Debug)]
enum SortCol {
    Keys,
    App,
    Action,
    Works,
    Found,
}

/// The order of the By app list.
#[derive(PartialEq, Clone, Copy)]
enum AppSort {
    Name,
    Count,
    Use,
}

enum Msg {
    Quick(ScanResult),
    Deep(ScanResult),
}

pub struct AtlasApp {
    quick: ScanResult,
    /// The last result of the slow sources. None until the first deep scan ends.
    deep: Option<ScanResult>,
    data: ScanResult,
    conflicts: Vec<Conflict>,
    rx: Option<mpsc::Receiver<Msg>>,
    loaded: bool,
    scanning: bool,
    tab: Tab,
    settings_open: bool,
    search: String,
    show_kind: HashMap<Kind, bool>,
    hide_standard: bool,
    include_inactive: bool,
    lookup: String,
    /// When the lookup text changed last, so VoiceOver reads the verdict after typing stops.
    lookup_edit: std::time::Instant,
    /// The verdict as VoiceOver reads it.
    lookup_spoken: String,
    lookup_sel: Option<usize>,
    /// The selected shortcut (`favorites::key`): a rescan or a new sort moves its row.
    lookup_sel_key: Option<String>,
    recording: bool,
    scope_filter: ScopeFilter,
    sort_col: SortCol,
    sort_desc: bool,
    /// All shortcuts: only apps that are open or had presses in 30 days.
    only_mine: bool,
    all_sel: Option<usize>,
    all_sel_key: Option<String>,
    /// ⌘F was pressed: the search field of the tab takes focus.
    focus_search: bool,
    expanded: HashSet<String>,
    // Screenshot mode.
    screenshot: Option<PathBuf>,
    frames: u32,
    shot: Arc<Mutex<Option<Arc<egui::ColorImage>>>>,
    shot_asked: bool,
    started: std::time::Instant,
    last_trust_check: std::time::Instant,
    install_msg: Option<String>,
    keyboard: crate::keyboard::KeyboardMap,
    selected_app: Option<String>,
    app_filter: String,
    app_sort: AppSort,
    /// Scroll the selected app into view in the list.
    app_reveal: bool,
    /// The filter in the By app pane, and the app it belongs to.
    pane_filter: String,
    pane_for: Option<String>,
    /// Presses in the last 30 days, by app id and keys, and when they were read.
    uses_month: HashMap<String, HashMap<Trigger, u32>>,
    uses_read: Option<std::time::Instant>,
    // Double-tap ⌘ panel.
    hk_rx: Option<mpsc::Receiver<crate::hotkey::HotkeyMsg>>,
    panel_open: Arc<std::sync::atomic::AtomicBool>,
    panel: Option<crate::panel::Panel>,
    /// Every trigger in use, to suggest free keys.
    used: HashSet<Trigger>,
    /// Installed apps that are not open now (bundle ids).
    not_running: HashSet<String>,
    export_msg: Option<String>,
    // Menus learned in the background when an app starts.
    learn_rx: Option<mpsc::Receiver<crate::learn::LearnMsg>>,
    /// What was learned last, and when, for a short note in the header.
    learned: Option<(String, std::time::Instant)>,
    /// When the macOS keyboard layout was checked last.
    last_layout_check: std::time::Instant,
    /// The layout changed: scan again, because hotkeys stored as key codes get new letters.
    layout_changed: bool,
    /// `--layout <name>` was given: keep that layout for this run.
    layout_fixed: bool,
    /// The "Use" tab, and the press counts for the keyboard map.
    use_view: crate::use_tab::UseView,
    /// Goes up by one when the shortcut list changes.
    scan_gen: u64,
    /// The app icon, for the header.
    logo: Option<egui::TextureHandle>,
    /// The welcome page is open (first start, or from Settings).
    show_welcome: bool,
    /// The tick box for the use counter on the welcome page.
    welcome_count: bool,
}

impl AtlasApp {
    pub fn new(cc: &eframe::CreationContext<'_>, screenshot: Option<PathBuf>) -> Self {
        let args: Vec<String> = std::env::args().collect();
        // `--theme light|dark|system`: use this theme for this run only.
        let forced = args.iter().position(|a| a == "--theme").and_then(|i| args.get(i + 1)).and_then(|t| theme::parse_pref(t));
        theme::setup(&cc.egui_ctx, forced);
        // Screenshot mode reads the files first so the first frames show real data.
        let loaded = screenshot.is_some();
        let quick = if loaded { scan::quick_scan() } else { ScanResult::default() };
        let data = scan::merge(&quick, None);
        let conflicts = conflicts::find(&data.shortcuts);
        let mut show_kind = HashMap::new();
        for k in KINDS {
            show_kind.insert(k, matches!(k, Kind::Clash | Kind::Blocks | Kind::SameModifier));
        }
        let mut app = AtlasApp {
            quick,
            deep: None,
            data,
            conflicts,
            rx: None,
            loaded,
            scanning: false,
            tab: Tab::Conflicts,
            settings_open: false,
            search: String::new(),
            show_kind,
            hide_standard: true,
            include_inactive: false,
            lookup: "cmd shift t".into(),
            lookup_edit: std::time::Instant::now(),
            lookup_spoken: String::new(),
            lookup_sel: None,
            lookup_sel_key: None,
            recording: false,
            scope_filter: ScopeFilter::All,
            sort_col: SortCol::Keys,
            sort_desc: false,
            only_mine: false,
            all_sel: None,
            all_sel_key: None,
            focus_search: false,
            expanded: HashSet::new(),
            screenshot,
            frames: 0,
            shot: Arc::new(Mutex::new(None)),
            shot_asked: false,
            started: std::time::Instant::now(),
            last_trust_check: std::time::Instant::now(),
            install_msg: None,
            keyboard: Default::default(),
            selected_app: None,
            app_filter: String::new(),
            app_sort: AppSort::Name,
            app_reveal: false,
            pane_filter: String::new(),
            pane_for: None,
            uses_month: HashMap::new(),
            uses_read: None,
            hk_rx: None,
            panel_open: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            panel: None,
            used: HashSet::new(),
            not_running: HashSet::new(),
            export_msg: None,
            learn_rx: None,
            learned: None,
            last_layout_check: std::time::Instant::now(),
            layout_changed: false,
            layout_fixed: args.iter().any(|a| a == "--layout"),
            use_view: Default::default(),
            scan_gen: 0,
            show_welcome: false,
            welcome_count: false,
            logo: crate::bundle::decode_png(crate::bundle::LOGO_PNG).map(|(rgba, w, h)| {
                let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                cc.egui_ctx.load_texture("logo", img, egui::TextureOptions::LINEAR)
            }),
        };
        // The welcome page: at the first start, and with `--welcome` (for a screenshot).
        let s = crate::settings::get();
        app.welcome_count = s.count_keys;
        app.show_welcome = (!s.welcomed && app.screenshot.is_none()) || args.iter().any(|a| a == "--welcome");
        // `--heat`: start the keyboard map with the Use colors.
        app.keyboard.heat = args.iter().any(|a| a == "--heat");
        app.reindex();
        // `--tab lookup|all|apps|sources` opens another tab first.
        if let Some(t) = args.iter().position(|a| a == "--tab").and_then(|i| args.get(i + 1)) {
            app.tab = match t.as_str() {
                "lookup" => Tab::Lookup,
                "keyboard" => Tab::Keyboard,
                "all" => Tab::All,
                "apps" => Tab::Apps,
                "use" => Tab::Usage,
                "sources" => Tab::Sources,
                "settings" => {
                    app.settings_open = true;
                    Tab::Conflicts
                }
                _ => Tab::Conflicts,
            };
        }
        if app.screenshot.is_some() && app.settings_open {
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(560.0, 420.0)));
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(680.0, 760.0)));
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Title("Keyclash Settings".into()));
        }
        // `--lookup <text>` and `--app <id>`: start Look up keys or By app with this text or app (screenshots).
        if let Some(l) = args.iter().position(|a| a == "--lookup").and_then(|i| args.get(i + 1)) {
            app.lookup = l.clone();
            app.lookup_edit = std::time::Instant::now() - std::time::Duration::from_secs(5);
        }
        if let Some(b) = args.iter().position(|a| a == "--app").and_then(|i| args.get(i + 1)) {
            app.selected_app = Some(b.clone());
        }
        if let Some(b) = args.iter().position(|a| a == "--panel").and_then(|i| args.get(i + 1)) {
            let front = crate::hotkey::FrontApp { bundle: b.clone(), pid: 0, name: String::new(), path: None };
            let mut p = crate::panel::Panel::open(front, &app.data.shortcuts, false, &HashSet::new());
            // `--hold "cmd shift"` and `--query <text>`: photograph Peek with keys held or a filter typed.
            if let Some(h) = args.iter().position(|a| a == "--hold").and_then(|i| args.get(i + 1)) {
                p.fake_held = h.split_whitespace().filter_map(crate::keys::modifier_word).fold(0, |a, b| a | b);
            }
            if let Some(q) = args.iter().position(|a| a == "--query").and_then(|i| args.get(i + 1)) {
                p.set_query(q);
            }
            p.wants_focus = true;
            app.set_panel(Some(p));
        }
        // `--lock "cmd shift"`: start the keyboard map with these modifiers switched on.
        if let Some(l) = args.iter().position(|a| a == "--lock").and_then(|i| args.get(i + 1)) {
            for w in l.split_whitespace() {
                if let Some(m) = crate::keys::modifier_word(w) {
                    app.keyboard.locked |= m;
                }
            }
        }
        if app.screenshot.is_none() {
            app.start_scan(&cc.egui_ctx);
            let (tx, rx) = mpsc::channel();
            crate::hotkey::start(tx, cc.egui_ctx.clone(), app.panel_open.clone());
            app.hk_rx = Some(rx);
            let (tx, rx) = mpsc::channel();
            crate::learn::start(tx, cc.egui_ctx.clone());
            app.learn_rx = Some(rx);
        }
        app
    }

    /// Read files, then app code and live menus, on a background thread.
    fn start_scan(&mut self, ctx: &egui::Context) {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let quick = scan::quick_scan();
            let _ = tx.send(Msg::Quick(quick.clone()));
            ctx.request_repaint();
            let deep = scan::deep_scan(&quick);
            let _ = tx.send(Msg::Deep(deep));
            ctx.request_repaint();
        });
        self.rx = Some(rx);
        self.scanning = true;
    }

    /// Build the list on screen again from the scan results.
    fn apply(&mut self) {
        self.data = scan::merge(&self.quick, self.deep.as_ref());
        self.conflicts = conflicts::find(&self.data.shortcuts);
        self.reindex();
    }

    /// Follow the macOS keyboard layout. Runs on the main thread, as macOS needs for these calls.
    fn follow_layout(&mut self, ctx: &egui::Context) {
        if self.screenshot.is_some() || self.layout_fixed {
            return;
        }
        if self.last_layout_check.elapsed().as_secs_f32() > 2.0 {
            self.last_layout_check = std::time::Instant::now();
            let now = crate::layout::current_id();
            if now.is_some() && now != crate::layout::active_id() {
                crate::layout::set(crate::layout::read_current());
                self.keyboard.selected = None;
                self.layout_changed = true;
            }
        }
        if self.layout_changed && !self.scanning {
            self.layout_changed = false;
            self.start_scan(ctx);
        }
    }

    /// Messages from the background watcher: which apps are open, and menus read from an app that started.
    fn poll_learn(&mut self) {
        let msgs: Vec<crate::learn::LearnMsg> = self.learn_rx.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        if msgs.is_empty() {
            return;
        }
        for m in msgs {
            match m {
                crate::learn::LearnMsg::Running(open) => {
                    for a in &mut self.quick.apps {
                        a.running = a.bundle.as_ref().is_some_and(|b| open.contains(b));
                    }
                }
                crate::learn::LearnMsg::Learned { name, bundle, shortcuts } => {
                    scan::remember_in(&mut self.quick, &bundle, &shortcuts, crate::sources::now_secs());
                    if let Some(d) = self.deep.as_mut() {
                        scan::add_live(d, &bundle, &shortcuts);
                    }
                    self.learned = Some((format!("Learned {} shortcuts from {name}", shortcuts.len()), std::time::Instant::now()));
                }
            }
        }
        self.apply();
    }

    /// Values worked out once per scan, not once per frame.
    fn reindex(&mut self) {
        self.scan_gen += 1;
        crate::icons::set_apps(&self.data.apps);
        self.used = crate::fix::used_triggers(&self.data.shortcuts);
        self.not_running = self.data.apps.iter().filter(|a| !a.running).filter_map(|a| a.bundle.clone()).collect();
    }

    /// True when this system-wide shortcut belongs to an app that is not open now.
    fn is_idle(&self, s: &Shortcut) -> bool {
        idle_tool(&self.not_running, s)
    }

    /// Write the clash list to the Downloads folder and show it in Finder.
    fn export_markdown(&mut self) {
        let path = crate::sources::home().join("Downloads/Keyclash report.md");
        let md = crate::report::markdown(&self.data.shortcuts, &self.conflicts);
        self.export_msg = Some(match std::fs::write(&path, md) {
            Ok(()) => {
                let _ = std::process::Command::new("/usr/bin/open").arg("-R").arg(&path).spawn();
                format!("Saved {}", path.display())
            }
            Err(e) => format!("Could not save {}: {e}", path.display()),
        });
    }

    fn matches_search(&self, s: &Shortcut) -> bool {
        let q = self.search.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        if let Some(t) = parse_typed(&q) {
            if t == s.trigger {
                return true;
            }
        }
        let hay = format!("{} {} {} {}", s.app, s.action, trigger_text(&s.trigger), trigger_words(&s.trigger)).to_lowercase();
        q.split_whitespace().all(|w| hay.contains(w))
    }

    fn count(&self, k: Kind) -> usize {
        self.conflicts
            .iter()
            .filter(|c| self.include_inactive || c.is_active(&self.data.shortcuts, &|s| self.is_idle(s)))
            .filter(|c| c.kind == k && !(k == Kind::Shared && self.hide_standard && conflicts::is_standard(&c.trigger)))
            .count()
    }

    // ------------------------------------------------------------ header

    fn header(&mut self, ui: &mut egui::Ui) {
        let t = th();
        let ctx = ui.ctx().clone();
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let (logo, _) = ui.allocate_exact_size(egui::vec2(38.0, 38.0), egui::Sense::hover());
            match &self.logo {
                Some(tex) => {
                    egui::Image::new(tex).paint_at(ui, logo);
                }
                None => {
                    ui.painter().rect_filled(logo, 10.0, t.accent);
                    ui.painter().text(logo.center(), egui::Align2::CENTER_CENTER, "⌘", egui::FontId::proportional(20.0), t.on_accent);
                }
            }
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(semi("Keyclash").size(18.0).color(t.text));
                let apps: HashSet<String> = self.data.shortcuts.iter().map(|s| s.owner()).collect();
                ui.label(
                    RichText::new(format!(
                        "{} shortcuts from {} apps and tools on this Mac",
                        num(self.data.shortcuts.iter().filter(|s| s.enabled).count()),
                        apps.len()
                    ))
                    .size(12.5)
                    .color(t.muted),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(!self.scanning, egui::Button::new(medium("Scan again"))).clicked() {
                    self.start_scan(&ctx);
                }
                if ui.button(medium("Settings…")).on_hover_text("Open Settings (⌘,)").clicked() {
                    self.open_settings(&ctx);
                }
                if self.scanning {
                    ui.add_space(4.0);
                    ui.label(RichText::new("Reading app code, app settings and live menus…").size(12.5).color(t.muted));
                    ui.spinner();
                } else if let Some((note, when)) = &self.learned {
                    // Show for a short time what the background watcher read.
                    let left = LEARNED_NOTE_SECS - when.elapsed().as_secs_f32();
                    if left > 0.0 {
                        ui.add_space(4.0);
                        ui.label(RichText::new(note).size(12.5).color(t.good))
                            .on_hover_text("Keyclash reads the menus of each app a few seconds after the app starts.");
                        dot(ui, t.good);
                        ctx.request_repaint_after(std::time::Duration::from_secs_f32(left));
                    }
                }
            });
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            for (k, label) in [
                (Kind::Clash, Kind::Clash.title()),
                (Kind::Blocks, Kind::Blocks.title()),
                (Kind::SameModifier, Kind::SameModifier.title()),
                (Kind::Shared, "Shared by apps"),
            ] {
                stat(ui, self.count(k), label, t.kind(k));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.panel_hint(ui));
        });
        if !self.data.ax_trusted {
            ui.add_space(10.0);
            self.permission_panel(ui);
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 20.0;
            for (tb, name) in [
                (Tab::Conflicts, "Conflicts"),
                (Tab::Keyboard, "Keyboard map"),
                (Tab::Lookup, "Look up keys"),
                (Tab::All, "All shortcuts"),
                (Tab::Apps, "By app"),
                (Tab::Usage, "Use"),
                (Tab::Sources, "Where we looked"),
            ] {
                if tab(ui, self.tab == tb, name) {
                    self.tab = tb;
                }
            }
            if matches!(self.tab, Tab::Conflicts | Tab::All) {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let r = ui
                        .add(
                            egui::TextEdit::singleline(&mut self.search)
                                .hint_text("Filter by app, action or keys (cmd t)")
                                .margin(egui::Margin::symmetric(9, 5))
                                .desired_width(270.0),
                        )
                        .on_hover_text("⌘F");
                    if r.changed() {
                        self.all_sel = None;
                    }
                    if self.focus_search {
                        self.focus_search = false;
                        r.request_focus();
                    }
                });
            }
        });
    }

    /// The ⌘ ⌘ panel reminder at the right of the header. The row runs right to left.
    fn panel_hint(&mut self, ui: &mut egui::Ui) {
        let t = th();
        let roomy = ui.available_width() > 430.0;
        let key = crate::settings::get().panel_key;
        let on = key != crate::settings::PanelKey::Off;
        let label = if roomy { "Try it" } else { crate::panel::NAME };
        let tip = if on {
            format!("Peek: press {} twice quickly in any app to see that app's shortcuts. This button opens Peek for the last app you used. Change the key in Settings.", key.symbol())
        } else {
            "The Peek key is off. This button opens Peek for the last app you used. Turn the key on in Settings.".to_string()
        };
        if ui.button(label).on_hover_text(tip).clicked()
        {
            if let Some(app) = crate::hotkey::target_app() {
                let mut p = crate::panel::Panel::open(app, &self.data.shortcuts, crate::ax::is_trusted(), &self.not_running);
                p.wants_focus = true;
                self.set_panel(Some(p));
            }
        }
        if roomy && on {
            ui.label(RichText::new("twice in any app to see its shortcuts").size(12.5).color(t.muted));
            cap(ui, key.symbol(), t.sym(key.symbol()), 11.5);
            ui.label(RichText::new("Press").size(12.5).color(t.muted));
        } else if roomy {
            ui.label(RichText::new(crate::panel::NAME).size(12.5).color(t.muted));
        }
        if self.screenshot.is_none() {
            ui.add_space(4.0);
            let (word, color, tip) = if crate::hotkey::TAP_ACTIVE.load(std::sync::atomic::Ordering::Relaxed) {
                ("listening", t.good, "The key listener is running. It reads each key press and gives it back unchanged. It also sees mouse-downs, not where you click, so a ⌘-click does not open Peek.")
            } else {
                (
                    "limited",
                    t.warn,
                    "macOS has not allowed the key listener yet, so quick taps can be missed. Allow Accessibility for Keyclash.",
                )
            };
            ui.label(RichText::new(word).size(12.0).color(color)).on_hover_text(tip);
            dot(ui, color);
        }
    }

    // ------------------------------------------------------------ permission

    /// Steps to turn on live menus. A plain program cannot be listed in Accessibility
    /// by its own name, so step 1 makes a real app first.
    fn permission_panel(&mut self, ui: &mut egui::Ui) {
        let t = th();
        let ctx = ui.ctx().clone();
        // While the panel shows, check now and then whether permission was given.
        if self.screenshot.is_none() && self.last_trust_check.elapsed().as_secs_f32() > 2.0 {
            self.last_trust_check = std::time::Instant::now();
            if crate::ax::is_trusted() {
                self.data.ax_trusted = true;
                self.quick.ax_trusted = true;
                self.install_msg = Some("Permission is on. Reading live menus now…".into());
                if !self.scanning {
                    self.start_scan(&ctx);
                }
                return;
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(2));
        }
        let as_app = crate::bundle::running_as_app();
        callout(t.warn).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(semi("Live menus are off.").color(t.warn));
                ui.label(
                    RichText::new("Menus now come from files inside each app. Turn on Accessibility to read the exact menus of open apps.").color(t.text),
                );
            });
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                step(ui, 1);
                if as_app {
                    if ui.button("Ask macOS for permission").on_hover_text("macOS adds Keyclash to the Accessibility list.").clicked() {
                        crate::ax::request_trust();
                    }
                    if ui
                        .button("Repair permission")
                        .on_hover_text("Use this when Keyclash is already ticked in Accessibility but live menus stay off. The tick belonged to an older version.")
                        .clicked()
                    {
                        self.install_msg = Some(crate::bundle::repair_permission());
                    }
                } else if ui
                    .button("Install as a Mac app")
                    .on_hover_text("Makes \"Keyclash\" in your Applications folder and opens it. Use that window from now on.")
                    .clicked()
                {
                    self.install_msg = Some(match crate::bundle::install_and_open() {
                        Ok(p) => format!(
                            "Installed {}. Use the new Keyclash window and press \"Ask macOS for permission\" there. You can close this one.",
                            p.display()
                        ),
                        Err(e) => format!("Could not install: {e}"),
                    });
                }
                ui.add_space(6.0);
                step(ui, 2);
                if ui.button("Open Accessibility settings").clicked() {
                    crate::bundle::open_accessibility_settings();
                }
                ui.add_space(6.0);
                step(ui, 3);
                ui.label(RichText::new("Turn on \"Keyclash\" in the list.").color(t.text));
                ui.add_space(6.0);
                step(ui, 4);
                if ui.button("Check again").clicked() {
                    self.last_trust_check = std::time::Instant::now() - std::time::Duration::from_secs(10);
                }
            });
            if let Some(m) = &self.install_msg {
                ui.label(RichText::new(m).color(t.good));
            } else if !as_app {
                ui.label(
                    RichText::new("This copy runs inside Craft Anything, so macOS cannot list it by name. Step 1 makes a real app first.")
                        .color(t.muted)
                        .size(12.0),
                );
            }
        });
    }

    // ------------------------------------------------------------ conflicts

    fn conflicts_tab(&mut self, ui: &mut egui::Ui) {
        let t = th();
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.include_inactive, "Include inactive conflicts")
                .on_hover_text("Show potential clashes involving tools that are not open. Active conflicts appear first.");
            if !self.include_inactive {
                ui.label(RichText::new("Showing conflicts involving active tools.").size(12.5).color(t.muted));
            }
        });
        ui.horizontal_wrapped(|ui| {
            for k in KINDS {
                let on = self.show_kind[&k];
                if chip(ui, on, &format!("{}  {}", k.title(), self.count(k)), t.kind(k)).on_hover_text(k.explain()).clicked() {
                    self.show_kind.insert(k, !on);
                }
            }
            if self.show_kind[&Kind::Shared] {
                ui.checkbox(&mut self.hide_standard, RichText::new("Hide standard Mac keys (⌘C, ⌘V…)").size(12.5).color(t.muted));
            }
            ui.add_space(6.0);
            if ui.button("Save as Markdown").on_hover_text("Write this clash list, with how to change each shortcut, to your Downloads folder.").clicked() {
                self.export_markdown();
            }
            if let Some(m) = &self.export_msg {
                ui.label(RichText::new(m).color(t.good).size(12.0));
            }
        });
        ui.add_space(8.0);
        let list = &self.data.shortcuts;
        let mut visible: Vec<&Conflict> = self
            .conflicts
            .iter()
            .filter(|c| self.include_inactive || c.is_active(list, &|s| self.is_idle(s)))
            .filter(|c| self.show_kind[&c.kind])
            .filter(|c| !(c.kind == Kind::Shared && self.hide_standard && conflicts::is_standard(&c.trigger)))
            .filter(|c| c.members.iter().any(|&i| self.matches_search(&list[i])) || c.label.to_lowercase().contains(&self.search.to_lowercase()))
            .collect();
        visible.sort_by_key(|c| !c.is_active(list, &|s| self.is_idle(s)));
        if visible.is_empty() {
            empty(ui, "No conflicts match these filters.", "Include inactive conflicts to inspect closed tools, choose another kind, or clear the filter box.");
            return;
        }
        let mut toggle = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for c in visible {
                let color = t.kind(c.kind);
                let blocker = if c.kind == Kind::Blocks {
                    c.members.iter().map(|&i| &list[i]).find(|s| s.enabled && s.scope == Scope::Global)
                } else {
                    None
                };
                let title = match blocker {
                    Some(s) if s.estimated || self.is_idle(s) => format!("May be blocked by {}", s.app),
                    Some(s) => format!("Blocked by {}", s.app),
                    None => c.kind.title().to_string(),
                };
                card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let action_w = (ui.available_width() - MEMBER_FIXED).clamp(170.0, 520.0);
                    ui.horizontal_wrapped(|ui| {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(3.0, 26.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, 1.5, color);
                        ui.add_space(2.0);
                        if c.kind == Kind::SameModifier {
                            let m = mod_symbols(c.trigger.mods()).join("");
                            cap(ui, &m, t.sym(&m), 15.0);
                            ui.label(medium(&c.label).size(14.0).color(t.text));
                        } else {
                            keycaps(ui, &c.trigger, 15.0);
                        }
                        ui.add_space(4.0);
                        pill(ui, &title, color);
                        if !c.is_active(list, &|s| self.is_idle(s)) {
                            pill(ui, "not active now", t.muted)
                                .on_hover_text("The required tools are not open together. This is a potential conflict.");
                        }
                        if blocker.is_none() {
                            ui.add(egui::Label::new(RichText::new(c.kind.explain()).color(t.muted).size(12.5)).truncate());
                        }
                    });
                    if let Some(s) = blocker {
                        ui.label(
                            RichText::new(format!(
                                "While {} is running with this system-wide shortcut enabled, it takes these keys before the app actions below.",
                                s.app
                            ))
                            .color(t.muted)
                            .size(12.5),
                        );
                        ui.label(RichText::new("Use an app's menu, or change one of the shortcuts.").color(t.muted).size(12.5));
                    }
                    ui.add_space(6.0);
                    let key = format!("{:?}{}", c.kind, c.label);
                    let open = self.expanded.contains(&key);
                    let limit = if open { usize::MAX } else { 8 };
                    let mut members: Vec<usize> = c.members.clone();
                    members.sort_by_key(|&i| (list[i].scope != Scope::Global, list[i].app.to_lowercase()));
                    egui::Grid::new(&key).num_columns(6).spacing([MEMBER_GAP, 2.0]).show(ui, |ui| {
                        for &i in members.iter().take(limit) {
                            member_row(ui, &list[i], c.kind == Kind::SameModifier, self.is_idle(&list[i]), action_w);
                            ui.end_row();
                        }
                    });
                    // Keys nobody uses, to move one side of the clash to.
                    if matches!(c.kind, Kind::Clash | Kind::Blocks) {
                        let free = crate::fix::free_keys_near(&c.trigger, &self.used, 5);
                        if !free.is_empty() {
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Free with the same modifier keys").color(t.muted).size(12.0))
                                    .on_hover_text("No app or tool we could read uses these. Move one side of the clash to one of them with its \"change\" link.");
                                for f in &free {
                                    keycaps(ui, f, 11.0);
                                }
                            });
                        }
                    }
                    if members.len() > 8 {
                        let txt = if open { "Show fewer".to_string() } else { format!("Show all {} apps", members.len()) };
                        if ui.link(RichText::new(txt).size(12.5).color(t.accent)).clicked() {
                            toggle = Some(key);
                        }
                    }
                });
                ui.add_space(4.0);
            }
        });
        if let Some(k) = toggle {
            if !self.expanded.remove(&k) {
                self.expanded.insert(k);
            }
        }
    }

    // ------------------------------------------------------------ lookup

    fn lookup_tab(&mut self, ui: &mut egui::Ui) {
        let t = th();
        let mut field = None;
        let mut new_lookup: Option<String> = None;
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(semi("Which apps use these keys?").size(16.0).color(t.text));
            ui.label(RichText::new("Type keys in words or symbols, or press Record and press the keys.").color(t.muted));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.lookup)
                        .desired_width(340.0)
                        .margin(egui::Margin::symmetric(10, 7))
                        .font(egui::FontId::proportional(17.0)),
                );
                if r.changed() {
                    self.lookup_edit = std::time::Instant::now();
                    self.lookup_sel = None;
                }
                field = Some(r.id);
                let rec = if self.recording { "Done" } else { "Record" };
                if ui.add(egui::Button::new(medium(rec)).selected(self.recording).min_size(egui::vec2(76.0, 34.0))).clicked() {
                    self.recording = !self.recording;
                    // Keys go to the recorder, not the text box.
                    ui.ctx().memory_mut(|m| m.stop_text_input());
                }
            });
            if self.recording {
                // Hold the modifiers, then press the key. Recording stays on, so you can try
                // another combination; Done or Esc (alone) stops it.
                let held = crate::hotkey::current_mods();
                let (got, egui_mods) = ui.ctx().input_mut(|i| {
                    let mut got = None;
                    i.events.retain(|e| match e {
                        egui::Event::Key { key, pressed: true, modifiers, .. } => {
                            got = Some((*key, *modifiers));
                            false
                        }
                        egui::Event::Text(_) => false,
                        _ => true,
                    });
                    (got, i.modifiers)
                });
                let mut mods = held;
                for (on, bit) in [(egui_mods.ctrl, CTRL), (egui_mods.alt, OPT), (egui_mods.shift, SHIFT), (egui_mods.mac_cmd || egui_mods.command, CMD)] {
                    if on {
                        mods |= bit;
                    }
                }
                if let Some((key, m)) = got {
                    for (on, bit) in [(m.ctrl, CTRL), (m.alt, OPT), (m.shift, SHIFT), (m.mac_cmd || m.command, CMD)] {
                        if on {
                            mods |= bit;
                        }
                    }
                    if key == egui::Key::Escape && mods == 0 {
                        self.recording = false;
                    } else {
                        let words: Vec<&str> = [(CTRL, "ctrl"), (OPT, "opt"), (SHIFT, "shift"), (CMD, "cmd"), (FN, "fn")]
                            .iter()
                            .filter(|(b, _)| mods & b != 0)
                            .map(|(_, w)| *w)
                            .collect();
                        // egui calls the forward-delete key "Delete" and the delete key "Backspace".
                        let name = if key == egui::Key::Delete { "forwarddelete" } else { key.name() };
                        self.lookup = format!("{} {}", words.join(" "), name).trim().to_string();
                        self.lookup_edit = std::time::Instant::now();
                        self.lookup_sel = None;
                    }
                }
                ui.horizontal(|ui| {
                    // Accent, not red: red means a clash in this window.
                    dot(ui, t.accent);
                    ui.label(RichText::new("Recording: hold the modifier keys, then press the key.").color(t.text2));
                    if mods != 0 {
                        ui.label(semi(format!("Holding {}", crate::keys::mod_symbols(mods).join(""))).color(t.accent));
                    }
                });
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
            }
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Try").size(12.5).color(t.muted));
                for ex in ["cmd t", "cmd ,", "cmd a", "ctrl opt cmd left", "opt tab", "double option", "hold fn", "cmd space"] {
                    if ui.add(egui::Button::new(RichText::new(ex).size(12.0).color(t.text2)).small()).clicked() {
                        new_lookup = Some(ex.into());
                    }
                }
            });
        });
        if self.focus_search {
            self.focus_search = false;
            if let Some(id) = field {
                ui.memory_mut(|m| m.request_focus(id));
            }
        }
        ui.add_space(8.0);
        let trig = match check_typed(&self.lookup) {
            Ok(tr) => tr,
            Err(e) if e.is_empty() => {
                empty(ui, "Type a key to look it up.", "For example: cmd shift t");
                self.apply_lookup(new_lookup);
                return;
            }
            Err(e) => {
                // A word that is not a key gives no verdict: a guess would answer for other keys.
                callout(t.warn).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let r = ui.label(semi(&e).size(14.5).color(t.warn));
                    ui.ctx().accesskit_node_builder(r.id, |n| n.set_live(egui::accesskit::Live::Polite));
                    ui.label(
                        RichText::new("Use modifier words (cmd, shift, opt, ctrl, fn) and one key, for example t, space, left, or f5.").color(t.muted),
                    );
                });
                self.apply_lookup(new_lookup);
                return;
            }
        };
        let list = &self.data.shortcuts;
        let mut hits: Vec<&Shortcut> = list
            .iter()
            .filter(|s| {
                s.trigger == trig
                    || matches!((trig.bare_modifier(), s.trigger.bare_modifier()), (Some(a), Some(b)) if a & b != 0)
            })
            .collect();
        hits.sort_by_key(|s| (s.scope != Scope::Global, !s.enabled, s.app.to_lowercase()));
        let owners: HashSet<String> = hits.iter().filter(|s| s.enabled).map(|s| s.owner()).collect();
        let globals: HashSet<String> = hits.iter().filter(|s| s.enabled && s.scope == Scope::Global).map(|s| s.owner()).collect();
        let n = owners.len();
        let enabled = || hits.iter().filter(|s| s.enabled);
        let names = |scope: Scope| -> String {
            let mut v: Vec<&str> = enabled().filter(|s| s.scope == scope).map(|s| s.app.as_str()).collect();
            v.sort_unstable();
            v.dedup();
            v.join(", ")
        };
        let mode_only = n > 0 && enabled().all(|s| s.scope == Scope::Mode);
        let (msg, col) = if hits.is_empty() {
            ("Free: no app or tool we could read uses these keys.".to_string(), t.good)
        } else if globals.len() >= 2 {
            (format!("Clash: {} system-wide shortcuts want these keys.", globals.len()), t.clash)
        } else if globals.len() == 1 && n >= 2 {
            ("A system-wide shortcut takes these keys before any app sees them.".to_string(), t.blocks)
        } else if n == 0 {
            ("Only shortcuts that are turned off use these keys.".to_string(), t.shared)
        } else if globals.len() == 1 {
            (format!("System-wide: {} takes these keys in every app.", names(Scope::Global)), t.shared)
        } else if mode_only {
            (format!("Works only inside a mode of {}.", names(Scope::Mode)), t.shared)
        } else if n == 1 {
            ("Used by 1 app, only while it is in front.".to_string(), t.shared)
        } else {
            (format!("Used by {n} apps, each only while it is in front."), t.shared)
        };
        // VoiceOver reads the verdict once typing stops, not at each letter.
        if self.lookup_edit.elapsed().as_secs_f32() > LOOKUP_SETTLE_SECS {
            self.lookup_spoken = format!("{}. {msg}", typed_text(&trig));
        } else {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f32(LOOKUP_SETTLE_SECS + 0.05));
        }
        let spoken = self.lookup_spoken.clone();
        let near = near_matches(&trig, list);
        let free = if hits.is_empty() { Vec::new() } else { crate::fix::free_keys_near(&trig, &self.used, 6) };
        callout(col).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                keycaps(ui, &trig, 19.0);
                ui.add_space(8.0);
                let r = ui.label(semi(&msg).size(14.5).color(if col == t.shared { t.text } else { col }));
                ui.ctx().accesskit_node_builder(r.id, |node| {
                    node.set_live(egui::accesskit::Live::Polite);
                    node.set_label(spoken.clone());
                });
            });
            if hits.is_empty() {
                ui.label(
                    RichText::new("Apps whose menus are built only in code can still use them. Open such an app and scan again with live menus on.")
                        .color(t.muted),
                );
            }
            if !near.is_empty() || !free.is_empty() {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if !near.is_empty() {
                        ui.label(RichText::new("Also used:").size(12.5).color(t.muted));
                        for (tr, k) in &near {
                            let text = format!("{}  {k} {}", trigger_text(tr), if *k == 1 { "app" } else { "apps" });
                            if ui.add(egui::Button::new(RichText::new(text).size(12.0).color(t.text2)).small()).on_hover_text("Look up these keys").clicked() {
                                new_lookup = Some(typed_text(tr));
                            }
                        }
                    }
                    if !free.is_empty() {
                        ui.add_space(10.0);
                        ui.label(RichText::new("Free with the same modifier keys:").size(12.5).color(t.muted))
                            .on_hover_text("No app or tool we could read uses these.");
                        for f in &free {
                            if ui.add(egui::Button::new(RichText::new(trigger_text(f)).size(12.0).color(t.good)).small()).on_hover_text("Look up these keys").clicked() {
                                new_lookup = Some(typed_text(f));
                            }
                        }
                    }
                });
            }
        });
        let mut act = None;
        if !hits.is_empty() {
            ui.add_space(8.0);
            // A column that says the same on every row is noise: leave it out.
            let works_varies = hits.iter().any(|s| s.scope != hits[0].scope);
            let font = list_font();
            card().inner_margin(egui::Margin::symmetric(8, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let cols = ListCols::fit(ui.available_width(), trig.bare_modifier().is_some(), works_varies, font);
                list_header(ui, &cols, None);
                let v = ListView { rows: &hits, cols, font, not_running: &self.not_running, id: "lookup" };
                act = followed_list(ui, &v, &mut self.lookup_sel, &mut self.lookup_sel_key);
            });
        }
        self.apply_lookup(new_lookup);
        if let Some(a) = act {
            self.row_act(a);
        }
    }

    /// Put new text in the lookup box (an example, a near match, a free key).
    fn apply_lookup(&mut self, text: Option<String>) {
        if let Some(l) = text {
            self.lookup = l;
            self.lookup_edit = std::time::Instant::now();
            self.lookup_sel = None;
        }
    }

    /// Do what a list row asked for.
    fn row_act(&mut self, a: RowAct) {
        match a {
            RowAct::ShowApp(owner) => {
                self.selected_app = Some(owner);
                self.app_filter.clear();
                self.app_reveal = true;
                self.tab = Tab::Apps;
            }
            RowAct::LookUp(tr) => {
                self.apply_lookup(Some(typed_text(&tr)));
                self.tab = Tab::Lookup;
            }
        }
    }

    /// Presses in the last 30 days, by app id and keys. Read again every few seconds.
    fn refresh_uses(&mut self) {
        if self.uses_read.is_some_and(|w| w.elapsed().as_secs() < 5) {
            return;
        }
        let mut m: HashMap<String, HashMap<Trigger, u32>> = HashMap::new();
        for (tr, b, n) in crate::usage::snapshot(crate::usage::Range::Month).1 {
            *m.entry(b).or_default().entry(tr).or_default() += n;
        }
        self.uses_month = m;
        self.uses_read = Some(std::time::Instant::now());
    }

    /// App ids that are open now or had presses in the last 30 days, with macOS.
    fn my_apps(&self) -> HashSet<String> {
        let mut out: HashSet<String> = self.data.apps.iter().filter(|a| a.running).filter_map(|a| a.bundle.clone()).collect();
        out.extend(self.uses_month.keys().cloned());
        out.insert(crate::sources::MACOS.to_string());
        out
    }

    // ------------------------------------------------------------ all

    fn all_tab(&mut self, ui: &mut egui::Ui) {
        let t = th();
        self.refresh_uses();
        let mine = self.my_apps();
        let (scope, only) = (self.scope_filter, self.only_mine);
        let mut rows: Vec<&Shortcut> = self
            .data
            .shortcuts
            .iter()
            .filter(|s| scope.keeps(s))
            .filter(|s| !only || is_mine(s, &mine))
            .filter(|s| self.matches_search(s))
            .collect();
        sort_rows(&mut rows, self.sort_col, self.sort_desc);
        let (mut scope_v, mut only_v) = (scope, only);
        let mut export = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Works").size(12.5).color(t.muted));
            segmented(
                ui,
                &mut scope_v,
                &[
                    (ScopeFilter::All, "All", ""),
                    (ScopeFilter::Global, "System-wide", "Works in every app"),
                    (ScopeFilter::App, "In apps", "Works only while its app is in front"),
                    (ScopeFilter::Mode, "In a mode", "Works only in a mode of a tool, such as an AeroSpace mode"),
                ],
            );
            ui.add_space(10.0);
            ui.checkbox(&mut only_v, RichText::new("Only my apps").size(12.5))
                .on_hover_text("Apps that are open now or had presses in the last 30 days, with macOS, key remappers, and tools without an app.");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("Export", |ui| {
                    if ui.button("Save as Markdown").on_hover_text("These rows, as a table, in your Downloads folder").clicked() {
                        export = Some(false);
                        ui.close();
                    }
                    if ui.button("Save as CSV").on_hover_text("These rows, for a spreadsheet, in your Downloads folder").clicked() {
                        export = Some(true);
                        ui.close();
                    }
                });
                ui.label(RichText::new(format!("{} shortcuts", num(rows.len()))).size(12.5).color(t.muted));
                if let Some(m) = &self.export_msg {
                    ui.add(egui::Label::new(RichText::new(m).color(t.good).size(12.0)).truncate());
                }
            });
        });
        if scope_v != scope || only_v != only {
            self.all_sel = None;
        }
        self.scope_filter = scope_v;
        self.only_mine = only_v;
        if rows.is_empty() {
            empty(ui, "No shortcuts match.", "Clear the filter box, choose All under Works, or turn off Only my apps.");
            return;
        }
        ui.add_space(6.0);
        let font = list_font();
        let (mut col, mut desc) = (self.sort_col, self.sort_desc);
        let mut act = None;
        card().inner_margin(egui::Margin::symmetric(8, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let cols = ListCols::fit(ui.available_width(), true, true, font);
            list_header(ui, &cols, Some((&mut col, &mut desc)));
            let v = ListView { rows: &rows, cols, font, not_running: &self.not_running, id: "all" };
            act = followed_list(ui, &v, &mut self.all_sel, &mut self.all_sel_key);
        });
        if (col, desc) != (self.sort_col, self.sort_desc) {
            self.all_sel = None;
        }
        self.sort_col = col;
        self.sort_desc = desc;
        if let Some(csv) = export {
            let (name, text) = if csv { ("Keyclash shortcuts.csv", list_csv(&rows)) } else { ("Keyclash shortcuts.md", list_markdown(&rows)) };
            self.export_msg = Some(save_download(name, &text));
        }
        if let Some(a) = act {
            self.row_act(a);
        }
    }

    // ------------------------------------------------------------ apps

    fn apps_tab(&mut self, ui: &mut egui::Ui) {
        let t = th();
        self.refresh_uses();
        // Owners with shortcuts: installed apps plus tools without an app (macOS, skhd…).
        // The count uses the same rows as the pane on the right (`panel::owner_items`).
        let mut per: HashMap<String, (String, usize)> = HashMap::new();
        for s in self.data.shortcuts.iter().filter(|s| s.enabled) {
            let e = per.entry(s.owner()).or_insert_with(|| (s.app.clone(), 0));
            e.1 += 1;
        }
        let open = |b: &str| b == crate::sources::MACOS || self.data.apps.iter().any(|a| a.bundle.as_deref() == Some(b) && a.running);
        let presses = |b: &str| self.uses_month.get(b).map(|m| m.values().sum::<u32>()).unwrap_or(0);
        let mut rows: Vec<AppEntry> =
            per.iter().map(|(b, (name, n))| AppEntry { id: b.clone(), name: name.clone(), count: *n, open: open(b), presses: presses(b) }).collect();
        // The id breaks a tie between equal names. Without it the map order, which changes
        // on every frame, decides, and the two rows swap places.
        let by_name = |a: &AppEntry, b: &AppEntry| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id));
        match self.app_sort {
            AppSort::Name => rows.sort_by(by_name),
            AppSort::Count => rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| by_name(a, b))),
            AppSort::Use => rows.sort_by(|a, b| b.presses.cmp(&a.presses).then_with(|| by_name(a, b))),
        }
        let none: Vec<&AppInfo> = self
            .data
            .apps
            .iter()
            .filter(|a| !a.background_only && !per.contains_key(&a.bundle.clone().unwrap_or_default()))
            .collect();
        let f = self.app_filter.to_lowercase();
        // Open apps first, then the rest. This is also the order of ↑ and ↓.
        let visible: Vec<&AppEntry> =
            rows.iter().filter(|r| r.open).chain(rows.iter().filter(|r| !r.open)).filter(|r| f.is_empty() || r.name.to_lowercase().contains(&f)).collect();
        if self.selected_app.is_none() {
            self.selected_app = rows.iter().find(|r| r.name == "macOS").or(rows.first()).map(|r| r.id.clone());
        }
        // The filter hides the selected app: select the first match, so the list and the pane agree.
        if !f.is_empty()
            && !visible.iter().any(|r| self.selected_app.as_deref() == Some(r.id.as_str()))
            && let Some(r) = visible.first()
        {
            self.selected_app = Some(r.id.clone());
        }
        let filter_id = egui::Id::unique("app-filter");
        let filter_focused = ui.memory(|m| m.has_focus(filter_id));
        if !visible.is_empty() && (filter_focused || !ui.ctx().egui_wants_keyboard_input()) {
            let pos = visible.iter().position(|r| self.selected_app.as_deref() == Some(r.id.as_str()));
            let last = visible.len() - 1;
            let (down, up, enter) = ui.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                )
            });
            let to = if down {
                Some(pos.map_or(0, |p| (p + 1).min(last)))
            } else if up {
                Some(pos.map_or(0, |p| p.saturating_sub(1)))
            } else if enter && pos.is_none() {
                Some(0)
            } else {
                None
            };
            if let Some(k) = to {
                self.selected_app = Some(visible[k].id.clone());
                self.app_reveal = true;
            }
        }
        // A new app gets an empty filter in its pane.
        if self.pane_for != self.selected_app {
            self.pane_filter.clear();
            self.pane_for = self.selected_app.clone();
        }
        let held = crate::hotkey::current_mods();
        let reveal = std::mem::take(&mut self.app_reveal);
        let mut open_peek = None;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            // Left: the apps.
            let side = 264.0;
            ui.allocate_ui_with_layout(egui::vec2(side, ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
                card().inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                    ui.set_width(side - 18.0);
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.app_filter)
                            .id(filter_id)
                            .hint_text("Find an app")
                            .margin(egui::Margin::symmetric(9, 5))
                            .desired_width(f32::INFINITY),
                    );
                    let r = r.on_hover_text("⌘F. ↑ and ↓ move through the list.");
                    if self.focus_search {
                        self.focus_search = false;
                        r.request_focus();
                    }
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.add_space(4.0);
                        ui.label(RichText::new("Sort").size(12.0).color(t.muted));
                        segmented(
                            ui,
                            &mut self.app_sort,
                            &[
                                (AppSort::Name, "Name", "By name"),
                                (AppSort::Count, "Shortcuts", "Most shortcuts first"),
                                (AppSort::Use, "Use", "Most presses in the last 30 days first (Use counter)"),
                            ],
                        );
                    });
                    ui.add_space(2.0);
                    egui::ScrollArea::vertical().id_salt("app-list").auto_shrink([false, false]).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        let (open_rows, closed_rows): (Vec<&AppEntry>, Vec<&AppEntry>) = visible.iter().partition(|r| r.open);
                        for (title, part) in [("Open now", open_rows), ("Not open", closed_rows)] {
                            if part.is_empty() {
                                continue;
                            }
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.add_space(4.0);
                                ui.label(overline(&format!("{title} · {}", part.len())));
                            });
                            for r in part {
                                let sel = self.selected_app.as_deref() == Some(r.id.as_str());
                                let resp = app_row(ui, &r.name, r.count, r.open, sel);
                                if resp.clicked() {
                                    self.selected_app = Some(r.id.clone());
                                }
                                if sel && reveal {
                                    resp.scroll_to_me(None);
                                }
                            }
                        }
                        if !none.is_empty() {
                            ui.add_space(12.0);
                            ui.horizontal(|ui| {
                                ui.add_space(4.0);
                                ui.label(overline("No shortcuts found yet"));
                            });
                            ui.horizontal(|ui| {
                                ui.add_space(4.0);
                                ui.label(RichText::new("Open the app once while live menus are on.").color(t.muted).size(11.5));
                            });
                            ui.add_space(2.0);
                            for a in none.iter().filter(|a| f.is_empty() || a.name.to_lowercase().contains(&f)) {
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.add(egui::Label::new(RichText::new(&a.name).color(t.muted).size(12.5)).truncate());
                                });
                            }
                        }
                    });
                });
            });
            // Right: the selected app's shortcuts, grouped like Peek.
            // The row is horizontal, so the card needs its own top-down layout.
            let room = ui.available_size();
            ui.allocate_ui_with_layout(room, egui::Layout::top_down(egui::Align::Min), |ui| {
                card().show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    let Some(b) = self.selected_app.clone() else { return };
                    let items = crate::panel::owner_items(&self.data.shortcuts, &b);
                    let taken = crate::panel::taken_by(&self.data.shortcuts, &b);
                    let name = items.first().map(|s| s.app.clone()).unwrap_or(b.clone());
                    let running = open(&b);
                    // Row 1: who, how many, filter, Peek.
                    ui.horizontal(|ui| {
                        crate::icons::show(ui, Some(&b), &name, 24.0);
                        ui.label(semi(&name).size(18.0).color(t.text));
                        ui.label(RichText::new(format!("{} shortcuts", items.len())).color(t.muted));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if b.contains('.') && b != crate::sources::MACOS {
                                let r = ui.add_enabled(running, egui::Button::new(medium("Open in Peek")));
                                let tip = if running {
                                    format!("Show Peek for {name}. From Peek you can run a shortcut in {name}.")
                                } else {
                                    format!("Open {name} first. Peek runs shortcuts in the open app.")
                                };
                                if r.on_hover_text(tip).on_disabled_hover_text(format!("Open {name} first. Peek runs shortcuts in the open app.")).clicked() {
                                    open_peek = Some(b.clone());
                                }
                            }
                            ui.add(
                                egui::TextEdit::singleline(&mut self.pane_filter)
                                    .hint_text(format!("Filter {name}"))
                                    .margin(egui::Margin::symmetric(9, 5))
                                    .desired_width(210.0),
                            );
                        });
                    });
                    // Row 2: the held keys and how they pick rows.
                    ui.horizontal(|ui| {
                        if held != 0 {
                            ui.label(semi(format!("Holding {}", crate::keys::mod_symbols(held).join(""))).color(t.accent));
                        } else {
                            ui.label(RichText::new("Hold ⌃ ⌥ ⇧ ⌘ to highlight").color(t.muted).size(12.0));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), crate::panel::mode_picker);
                    });
                    ui.add_space(6.0);
                    let uses = self.uses_month.get(&b).cloned().unwrap_or_default();
                    crate::panel::draw_groups(ui, &items, &taken, held, &self.pane_filter, &uses);
                });
            });
        });
        if let Some(front) = open_peek.and_then(|b| crate::hotkey::running_app(&b)) {
            let mut p = crate::panel::Panel::open(front, &self.data.shortcuts, crate::ax::is_trusted(), &self.not_running);
            p.wants_focus = true;
            self.set_panel(Some(p));
        }
        if held != 0 || ui.ui_contains_pointer() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
        }
    }

    // ------------------------------------------------------------ sources

    fn sources_tab(&mut self, ui: &mut egui::Ui) {
        let t = th();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(semi("Where shortcuts were read from").size(16.0).color(t.text));
                ui.add_space(6.0);
                egui::Grid::new("src").num_columns(4).min_col_width(8.0).spacing([12.0, 8.0]).show(ui, |ui| {
                    for n in &self.data.notes {
                        let color = if n.ok { t.good } else { t.warn };
                        dot(ui, color);
                        ui.label(medium(&n.name).color(t.text));
                        ui.label(semi(num(n.count)).color(color));
                        ui.label(RichText::new(&n.status).color(t.muted));
                        ui.end_row();
                    }
                    if self.scanning {
                        ui.spinner();
                        ui.label(medium("App code, sandboxed settings, live menus").color(t.text));
                        ui.label("");
                        ui.label(RichText::new("still reading…").color(t.muted));
                        ui.end_row();
                    }
                });
                // macOS guards the settings of sandboxed apps. The person can allow Keyclash to read them.
                if self.data.notes.iter().any(|n| !n.ok && n.name == "Sandboxed app settings") {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Open Full Disk Access settings").on_hover_text("Turn on Keyclash in the list, then press \"Scan again\". Keyclash only reads the settings files. It works without this permission, but it then cannot see the hotkeys of sandboxed apps.").clicked() {
                            let _ = std::process::Command::new("/usr/bin/open").arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles").spawn();
                        }
                        ui.label(RichText::new("Optional. It adds the hotkeys of sandboxed apps to the list.").size(12.5).color(t.muted));
                    });
                }
            });
            ui.add_space(4.0);
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(semi("What this cannot see").size(16.0).color(t.text));
                ui.add_space(4.0);
                for line in [
                    "Menus that an app builds only in its code (SwiftUI, Chrome, many Microsoft apps) until the app runs and live menus are allowed.",
                    "Hotkeys that apps keep in their own databases (for example Raycast) or in sandboxed settings macOS does not share.",
                    "Shortcuts inside web pages and terminal programs.",
                    "Items marked \"guess\" come from app code or known defaults, not from your settings.",
                ] {
                    ui.horizontal_top(|ui| {
                        dot(ui, t.faint);
                        ui.label(RichText::new(line).color(t.text2));
                    });
                }
            });
        });
    }
}

/// Width of the fixed columns of a member row, with the gaps between all six columns.
const MEMBER_GAP: f32 = 14.0;
const MEMBER_FIXED: f32 = 170.0 + 100.0 + 140.0 + 92.0 + 120.0 + 5.0 * MEMBER_GAP;

/// True when this system-wide shortcut belongs to an app that is not open now.
/// Key remappers run as a background service, so they are never marked.
fn idle_tool(not_running: &HashSet<String>, s: &Shortcut) -> bool {
    s.scope == Scope::Global && s.origin != Origin::KeyRemap && s.bundle.as_ref().is_some_and(|b| not_running.contains(b))
}

/// A count with its label, in the header.
fn stat(ui: &mut egui::Ui, n: usize, label: &str, color: Color32) {
    let t = th();
    egui::Frame::new()
        .fill(t.bg)
        .stroke(egui::Stroke::new(1.0, t.line))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(14, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(semi(n.to_string()).size(20.0).color(if n == 0 { t.faint } else { color }));
                ui.label(RichText::new(label).size(13.0).color(t.text2));
            });
        });
}

/// A numbered step in the permission notice.
fn step(ui: &mut egui::Ui, n: u8) {
    let t = th();
    let (r, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
    ui.painter().circle_filled(r.center(), 10.0, theme::mix(t.surface, t.warn, 0.28));
    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, n.to_string(), egui::FontId::proportional(12.0), t.text);
}

/// A centred message for a list with nothing in it.
fn empty(ui: &mut egui::Ui, title: &str, hint: &str) {
    let t = th();
    ui.add_space(48.0);
    ui.vertical_centered(|ui| {
        ui.label(medium(title).size(16.0).color(t.text2));
        ui.label(RichText::new(hint).color(t.muted));
    });
}

/// One app in the list of the By app tab.
fn app_row(ui: &mut egui::Ui, name: &str, n: usize, running: bool, selected: bool) -> egui::Response {
    let t = th();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::click());
    let spoken = format!("{name}, {n} shortcuts, {}", if running { "open now" } else { "not open" });
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::ListItem, true, selected, &spoken));
    if ui.is_rect_visible(rect) {
        if selected {
            ui.painter().rect_filled(rect, 7.0, t.tint(t.accent, if t.dark { 0.22 } else { 0.13 }));
        } else if resp.hovered() {
            ui.painter().rect_filled(rect, 7.0, t.raised);
        }
        let count = ui.painter().layout_no_wrap(n.to_string(), egui::FontId::proportional(12.0), if selected { t.text2 } else { t.muted });
        let count_x = rect.right() - 10.0 - count.size().x;
        ui.painter().galley(egui::pos2(count_x, rect.center().y - count.size().y / 2.0), count, t.muted);
        let text = if selected { semi(name) } else { RichText::new(name) }.size(13.5).color(if selected { t.text } else { t.text2 });
        let g = theme::fit(ui, text, count_x - rect.left() - 54.0);
        ui.painter().galley(egui::pos2(rect.left() + 44.0, rect.center().y - g.size().y / 2.0), g, t.text);
        let icon = egui::Rect::from_center_size(egui::pos2(rect.left() + 27.0, rect.center().y), egui::vec2(18.0, 18.0));
        crate::icons::paint(ui, icon, None, name);
        // A green dot marks an app that is open now. The "Open now" heading above says it in words.
        if running {
            ui.painter().circle_filled(egui::pos2(rect.left() + 8.0, rect.center().y), 3.0, t.good);
        }
    }
    theme::focus_ring(ui, &resp, 7);
    resp.on_hover_text(if running { "Open now" } else { "Not open now" })
}

/// Where a shortcut works: a tag for system-wide, plain words for the rest.
fn scope_tag(ui: &mut egui::Ui, s: &Shortcut) {
    let t = th();
    match s.scope {
        Scope::Global => {
            pill(ui, "System-wide", t.accent).on_hover_text("Works in every app.");
        }
        Scope::App => {
            ui.label(RichText::new("In app").color(t.muted).size(12.0)).on_hover_text("Works only while this app is in front.");
        }
        Scope::Mode => {
            ui.label(RichText::new("In a mode").color(t.muted).size(12.0));
        }
    }
}

fn member_row(ui: &mut egui::Ui, s: &Shortcut, show_keys: bool, idle: bool, action_w: f32) {
    let t = th();
    let fg = if s.enabled { t.text } else { t.muted };
    fixed(ui, 170.0, |ui| {
        crate::icons::show(ui, s.bundle.as_deref(), &s.app, 18.0);
        ui.add(egui::Label::new(semi(&s.app).color(fg)).truncate());
    });
    fixed(ui, action_w, |ui| {
        if show_keys {
            keycaps(ui, &s.trigger, 11.5);
        }
        ui.add(egui::Label::new(RichText::new(&s.action).color(fg)).truncate()).on_hover_text(&s.action);
    });
    fixed(ui, 100.0, |ui| scope_tag(ui, s));
    fixed(ui, 140.0, |ui| {
        ui.add(egui::Label::new(RichText::new(s.origin.label()).color(t.muted).size(12.0)).truncate()).on_hover_text(&s.detail);
    });
    fixed(ui, 92.0, |ui| {
        if s.estimated {
            guess_tag(ui);
        }
        if !s.enabled {
            ui.label(RichText::new("off").color(t.muted).size(11.5));
        }
        if idle {
            ui.label(RichText::new("not open").color(t.muted).size(11.5)).on_hover_text("This app is not open now. Its keys only work while it runs.");
        }
    });
    // Where to change it.
    let fix = crate::fix::fix_for(s);
    if fix.action == crate::fix::FixAction::Nothing {
        ui.label("");
    } else if ui.link(RichText::new(format!("{} ›", fix.label)).color(t.accent).size(12.0)).on_hover_text(&fix.how).clicked() {
        crate::fix::run(&fix.action);
    }
}

/// A left-aligned cell of a fixed width.
fn fixed(ui: &mut egui::Ui, w: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(egui::vec2(w, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_width(w);
        add(ui);
    });
}

/// Width of the "guess" tag, for rows that leave room for it.
pub const GUESS_W: f32 = 46.0;

/// The tag for a shortcut read from app code or a known default. Every list uses this one style.
pub fn guess_tag(ui: &mut egui::Ui) -> egui::Response {
    let t = th();
    ui.add(egui::Label::new(RichText::new("guess").color(t.warn).size(11.5)).selectable(false))
        .on_hover_text("Read from app code or a known default, not from your settings.")
}

/// The text size of the shortcut lists: the Peek size, so ⌘ + and ⌘ − change both.
fn list_font() -> f32 {
    crate::settings::get().panel_font + 0.5
}

/// After this many seconds without typing, VoiceOver reads the lookup verdict.
const LOOKUP_SETTLE_SECS: f32 = 0.6;

/// Other modifier keys with the same key, and how many apps use each: ⌘T and ⌥⌘T for ⇧⌘T.
fn near_matches(t: &Trigger, list: &[Shortcut]) -> Vec<(Trigger, usize)> {
    let Trigger::Combo { mods, key } = t else { return Vec::new() };
    let mut owners: HashMap<&Trigger, HashSet<String>> = HashMap::new();
    for s in list.iter().filter(|s| s.enabled) {
        if let Trigger::Combo { mods: m, key: k } = &s.trigger
            && k == key
            && m != mods
        {
            owners.entry(&s.trigger).or_default().insert(s.owner());
        }
    }
    let mut v: Vec<(Trigger, usize)> = owners.into_iter().map(|(t, o)| (t.clone(), o.len())).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v.truncate(4);
    v
}

/// A shortcut of an app in `mine`, or of macOS, a key remapper, or a tool without an app.
fn is_mine(s: &Shortcut, mine: &HashSet<String>) -> bool {
    s.origin == Origin::KeyRemap || s.bundle.as_ref().is_none_or(|b| mine.contains(b))
}

/// Modifier keys in macOS menu order: none, ⌃, ⌥, ⇧, ⌘, then pairs, then three, then four.
fn mods_rank(m: u8) -> (u32, Vec<u8>) {
    let idx: Vec<u8> = [CTRL, OPT, SHIFT, CMD, FN].iter().enumerate().filter(|(_, b)| m & **b != 0).map(|(i, _)| i as u8).collect();
    (idx.len() as u32, idx)
}

type KeyRank = (bool, (u32, Vec<u8>), u8, bool, String);

/// Sort order of keys: mode keys last, then the modifier group, then shortcuts with a key before
/// modifier-only ones, then one-character keys before named keys, then the key.
fn key_rank(s: &Shortcut) -> KeyRank {
    let (kind, key) = match &s.trigger {
        Trigger::Combo { key, .. } => (0, key.clone()),
        Trigger::ModTap { taps, .. } => (1, taps.to_string()),
        Trigger::ModHold { .. } => (2, String::new()),
    };
    (s.scope == Scope::Mode, mods_rank(s.trigger.mods()), kind, key.chars().count() > 1, key)
}

fn sort_rows(rows: &mut Vec<&Shortcut>, col: SortCol, desc: bool) {
    let works = |s: &Shortcut| match s.scope {
        Scope::Global => "0",
        Scope::App => "1",
        Scope::Mode => "2",
    };
    let mut keyed: Vec<(String, KeyRank, String, &Shortcut)> = rows
        .iter()
        .map(|s| {
            let primary = match col {
                SortCol::Keys => String::new(),
                SortCol::App => s.app.to_lowercase(),
                SortCol::Action => s.action.to_lowercase(),
                SortCol::Works => works(s).to_string(),
                SortCol::Found => s.origin.label().to_string(),
            };
            (primary, key_rank(s), s.app.to_lowercase(), *s)
        })
        .collect();
    keyed.sort_by(|a, b| {
        let o = if col == SortCol::Keys { a.1.cmp(&b.1) } else { a.0.cmp(&b.0) };
        let o = if desc { o.reverse() } else { o };
        o.then_with(|| a.1.cmp(&b.1)).then_with(|| a.2.cmp(&b.2))
    });
    *rows = keyed.into_iter().map(|k| k.3).collect();
}

fn works_word(s: &Shortcut) -> &'static str {
    match s.scope {
        Scope::Global => "System-wide",
        Scope::App => "In app",
        Scope::Mode => "In a mode",
    }
}

/// The rows as a Markdown table.
fn list_markdown(rows: &[&Shortcut]) -> String {
    let cell = |s: &str| s.replace('|', "\\|");
    let mut out = format!("# Keyclash shortcuts\n\n{} shortcuts.\n\n| Keys | App | Action | Works | Found in |\n|---|---|---|---|---|\n", rows.len());
    for s in rows {
        let works = if s.enabled { works_word(s) } else { "Off" };
        let found = if s.estimated { format!("{} (guess)", s.origin.label()) } else { s.origin.label().to_string() };
        out.push_str(&format!("| {} | {} | {} | {} | {} |\n", cell(&trigger_text(&s.trigger)), cell(&s.app), cell(&s.action), works, cell(&found)));
    }
    out
}

/// The rows as CSV, for a spreadsheet.
fn list_csv(rows: &[&Shortcut]) -> String {
    let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut out = String::from("Keys,Key words,App,Action,Works,Found in,Guess,On\n");
    for s in rows {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            q(&trigger_text(&s.trigger)),
            q(&typed_text(&s.trigger)),
            q(&s.app),
            q(&s.action),
            works_word(s),
            q(s.origin.label()),
            if s.estimated { "yes" } else { "no" },
            if s.enabled { "yes" } else { "no" },
        ));
    }
    out
}

/// Write a file to the Downloads folder and show it in Finder. Returns the note for the window.
fn save_download(name: &str, text: &str) -> String {
    let path = crate::sources::home().join("Downloads").join(name);
    match std::fs::write(&path, text) {
        Ok(()) => {
            let _ = std::process::Command::new("/usr/bin/open").arg("-R").arg(&path).spawn();
            format!("Saved {}", path.display())
        }
        Err(e) => format!("Could not save {}: {e}", path.display()),
    }
}

/// What a list row asks the tab to do.
enum RowAct {
    ShowApp(String),
    LookUp(Trigger),
}

/// Columns of a shortcut list, with their widths. None leaves the column out.
#[derive(Clone, Copy)]
struct ListCols {
    keys: Option<f32>,
    app: f32,
    action: f32,
    works: Option<f32>,
    found: f32,
    fix: f32,
}

const LIST_PAD: f32 = 10.0;
const LIST_GAP: f32 = 8.0;

impl ListCols {
    /// Fixed widths for every column; the action takes the room that is left.
    fn fit(width: f32, keys: bool, works: bool, font: f32) -> ListCols {
        let k = (font / 13.5).max(0.8);
        let keys = keys.then_some(150.0 * k);
        let works = works.then_some(110.0 * k);
        let (app, found, fix) = (180.0 * k, 180.0 * k, 110.0 * k);
        let n = 4 + keys.is_some() as usize + works.is_some() as usize;
        let fixed = keys.unwrap_or(0.0) + works.unwrap_or(0.0) + app + found + fix + (n - 1) as f32 * LIST_GAP + 2.0 * LIST_PAD;
        ListCols { keys, app, action: (width - fixed).max(160.0), works, found, fix }
    }

    /// The columns in order: name, width, and the sort it stands for.
    fn names(&self) -> Vec<(&'static str, f32, Option<SortCol>)> {
        let mut v = Vec::new();
        if let Some(w) = self.keys {
            v.push(("Keys", w, Some(SortCol::Keys)));
        }
        v.push(("App", self.app, Some(SortCol::App)));
        v.push(("Action", self.action, Some(SortCol::Action)));
        if let Some(w) = self.works {
            v.push(("Works", w, Some(SortCol::Works)));
        }
        v.push(("Found in", self.found, Some(SortCol::Found)));
        v.push(("", self.fix, None));
        v
    }
}

/// The column names of a shortcut list. With `sort`, a click on a name sorts by that column,
/// and a second click turns the order around.
fn list_header(ui: &mut egui::Ui, c: &ListCols, mut sort: Option<(&mut SortCol, &mut bool)>) {
    let t = th();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = LIST_GAP;
        ui.add_space(LIST_PAD);
        for (name, w, col) in c.names() {
            fixed(ui, w, |ui| match (col, sort.as_mut()) {
                (Some(col), Some((cur, desc))) => {
                    let on = **cur == col;
                    let arrow = if !on {
                        ""
                    } else if **desc {
                        " ▼"
                    } else {
                        " ▲"
                    };
                    let text = overline(&format!("{name}{arrow}"));
                    let r = ui.add(egui::Label::new(if on { text.color(t.text2) } else { text }).sense(egui::Sense::click()).selectable(false));
                    let label = format!("{name}{}", if on { if **desc { ", sorted down" } else { ", sorted up" } } else { "" });
                    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::ColumnHeader, true, on, &label));
                    theme::focus_ring(ui, &r, 4);
                    if r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("Sort by {}", name.to_lowercase())).clicked() {
                        if on {
                            **desc = !**desc;
                        } else {
                            **cur = col;
                            **desc = false;
                        }
                    }
                }
                _ => {
                    ui.label(overline(name));
                }
            });
        }
    });
    let (line, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(line, 0.0, t.line);
}

/// Everything a shortcut list needs to draw.
struct ListView<'a> {
    rows: &'a [&'a Shortcut],
    cols: ListCols,
    font: f32,
    not_running: &'a HashSet<String>,
    /// Keeps the scroll place and selection of each list apart.
    id: &'a str,
}

/// A shortcut list: a click selects a row, ↑ ↓ move, ↩ shows the app in By app, Esc clears,
/// a right-click opens the row menu. Each row is one item for VoiceOver.
/// The line below the list holds the actions of the selected row, for the keyboard.
/// `shortcut_list` with a selection that stays on its shortcut when the rows change under it
/// (a background menu read, a new sort). `key` holds the selected shortcut's `favorites::key`.
fn followed_list(ui: &mut egui::Ui, v: &ListView, sel: &mut Option<usize>, key: &mut Option<String>) -> Option<RowAct> {
    if let Some(k) = key.as_deref() {
        let same = sel.and_then(|i| v.rows.get(i)).is_some_and(|s| crate::favorites::key(s) == k);
        if !same {
            *sel = v.rows.iter().position(|s| crate::favorites::key(s) == k);
        }
    }
    let act = shortcut_list(ui, v, sel);
    *key = sel.and_then(|i| v.rows.get(i)).map(|s| crate::favorites::key(s));
    act
}

fn shortcut_list(ui: &mut egui::Ui, v: &ListView, sel: &mut Option<usize>) -> Option<RowAct> {
    let t = th();
    let mut act = None;
    let row_h = (v.font * 2.2).round();
    if sel.is_some_and(|s| s >= v.rows.len()) {
        *sel = None;
    }
    let mut reveal = false;
    if !v.rows.is_empty() {
        let typing = ui.ctx().egui_wants_keyboard_input();
        let last = v.rows.len() - 1;
        ui.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                *sel = Some(sel.map_or(0, |s| (s + 1).min(last)));
                reveal = true;
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                *sel = Some(sel.map_or(0, |s| s.saturating_sub(1)));
                reveal = true;
            }
            if !typing {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                    *sel = None;
                }
                if let Some(s) = *sel
                    && i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                {
                    act = Some(RowAct::ShowApp(v.rows[s].owner()));
                }
            }
        });
    }
    let strip_h = 34.0;
    let list_h = (ui.available_height() - strip_h).max(row_h * 3.0);
    let mem = ui.scope_id().with(v.id).with("view");
    let (prev_off, prev_h): (f32, f32) = ui.data(|d| d.get_temp(mem)).unwrap_or((0.0, list_h));
    let mut area = egui::ScrollArea::vertical().id_salt(v.id).auto_shrink([false, false]).max_height(list_h);
    if let (true, Some(s)) = (reveal, *sel) {
        let top = s as f32 * row_h;
        if top < prev_off {
            area = area.vertical_scroll_offset(top);
        } else if top + row_h > prev_off + prev_h {
            area = area.vertical_scroll_offset((top + row_h - prev_h).max(0.0));
        }
    }
    let out = ui
        .scope(|ui| {
            // No gap between rows, so the stripes touch and the row maths of the list stays exact.
            ui.spacing_mut().item_spacing.y = 0.0;
            area.show_rows(ui, row_h, v.rows.len(), |ui, range| {
                for i in range {
                    if let Some(a) = list_row(ui, v, i, row_h, sel) {
                        act = Some(a);
                    }
                }
            })
        })
        .inner;
    ui.data_mut(|d| d.insert_temp(mem, (out.state.clamped_offset().y, out.inner_rect.height())));
    // The actions of the selected row, so the keyboard can reach them.
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.set_min_height(strip_h - 6.0);
        ui.add_space(LIST_PAD - 8.0);
        match sel.and_then(|i| v.rows.get(i)) {
            Some(s) => {
                keycaps(ui, &s.trigger, 11.5);
                ui.add(egui::Label::new(RichText::new(format!("{}: {}", s.app, s.action)).size(12.5).color(t.text2)).truncate().selectable(false));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Right to left: add in reverse so the buttons read in menu order.
                    if let Some(a) = row_actions(ui, s, false, true) {
                        act = Some(a);
                    }
                });
            }
            None => {
                ui.label(RichText::new("↑ ↓ select a row · Return shows its app · right-click for more").size(12.0).color(t.muted));
            }
        }
    });
    act
}

/// Star, hide, copy, and the jumps to other tabs, for one shortcut. In a menu each item closes it.
fn row_actions(ui: &mut egui::Ui, s: &Shortcut, in_menu: bool, reversed: bool) -> Option<RowAct> {
    let mut act = None;
    let starred = crate::favorites::is_starred(s);
    let hidden = crate::favorites::is_hidden(s);
    let fix = crate::fix::fix_for(s);
    let mut items: Vec<(String, String, u8)> = vec![
        ((if starred { "Unstar" } else { "Star" }).into(), "Starred shortcuts come first in Peek.".into(), 0),
        ((if hidden { "Show in Peek again" } else { "Hide in Peek" }).into(), "Hidden rows move to the Hidden group in Peek and By app.".into(), 1),
        ("Copy keys".into(), "Copy the key symbols, for example ⇧⌘T.".into(), 2),
        (format!("Show {} in By app", s.app), "All shortcuts of this app, grouped by menu.".into(), 3),
        ("Look up these keys".into(), "Which other apps use the same keys.".into(), 4),
    ];
    if fix.action != crate::fix::FixAction::Nothing {
        items.push((format!("{} …", fix.label), fix.how.clone(), 5));
    }
    if reversed {
        items.reverse();
    }
    for (label, tip, k) in items {
        let b = if in_menu { ui.button(label) } else { ui.add(egui::Button::new(RichText::new(label).size(12.0)).small()) };
        if b.on_hover_text(tip).clicked() {
            match k {
                0 => crate::favorites::toggle(s),
                1 => crate::favorites::toggle_hidden(s),
                2 => ui.ctx().copy_text(trigger_text(&s.trigger)),
                3 => act = Some(RowAct::ShowApp(s.owner())),
                4 => act = Some(RowAct::LookUp(s.trigger.clone())),
                _ => crate::fix::run(&fix.action),
            }
            if in_menu {
                ui.close();
            }
        }
    }
    act
}

/// The words VoiceOver reads for a row.
fn row_words(s: &Shortcut, idle: bool) -> String {
    let mut w = format!("{}, {}, {}", trigger_words(&s.trigger), s.app, s.action);
    w.push_str(&format!(", {}", if s.enabled { works_word(s).to_lowercase() } else { "off".into() }));
    w.push_str(&format!(", {}", s.origin.label()));
    if s.estimated {
        w.push_str(", guess");
    }
    if idle {
        w.push_str(", not open");
    }
    if crate::favorites::is_starred(s) {
        w.push_str(", starred");
    }
    w
}

/// A cell of a list row: the full row height, so every cell centres on the same line.
fn cell(ui: &mut egui::Ui, h: f32, w: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(egui::vec2(w, h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_width(w);
        ui.set_height(h);
        add(ui);
    });
}

/// One row of a shortcut list.
fn list_row(ui: &mut egui::Ui, v: &ListView, i: usize, row_h: f32, sel: &mut Option<usize>) -> Option<RowAct> {
    let t = th();
    let s = v.rows[i];
    let selected = *sel == Some(i);
    let idle = idle_tool(v.not_running, s);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Row, true, selected, row_words(s, idle)));
    if !ui.is_rect_visible(rect) {
        return None;
    }
    let hovered = ui.rect_contains_pointer(rect);
    if selected {
        ui.painter().rect(rect, 6.0, t.tint(t.accent, 0.14), egui::Stroke::new(1.0, t.tint(t.accent, 0.5)), egui::StrokeKind::Inside);
    } else if hovered {
        ui.painter().rect_filled(rect, 6.0, t.raised);
    } else if i % 2 == 1 {
        ui.painter().rect_filled(rect, 6.0, theme::mix(t.card, t.text, 0.028));
    }
    let mut act = None;
    let fg = if s.enabled { t.text } else { t.muted };
    let small = v.font - 1.5;
    let c = v.cols;
    let mut row = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(LIST_PAD, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let ui = &mut row;
    ui.spacing_mut().item_spacing.x = LIST_GAP;
    if let Some(w) = c.keys {
        cell(ui, row_h, w, |ui| keycaps(ui, &s.trigger, v.font - 1.0));
    }
    // The app name goes to the app in By app.
    cell(ui, row_h, c.app, |ui| {
        let r = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                crate::icons::show(ui, s.bundle.as_deref(), &s.app, v.font + 4.5);
                ui.add(egui::Label::new(semi(&s.app).size(v.font).color(fg)).truncate().selectable(false));
            })
            .response
            .interact(egui::Sense::click());
        if r.hovered() {
            let line = egui::Rect::from_min_max(egui::pos2(r.rect.left() + v.font + 10.5, r.rect.bottom() - 4.0), egui::pos2(r.rect.right(), r.rect.bottom() - 3.0));
            ui.painter().rect_filled(line, 0.0, t.tint(fg, 0.5));
        }
        if r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("Show {} in By app", s.app)).clicked() {
            act = Some(RowAct::ShowApp(s.owner()));
        }
    });
    cell(ui, row_h, c.action, |ui| {
        ui.add(egui::Label::new(RichText::new(&s.action).size(v.font).color(fg)).truncate().selectable(false)).on_hover_text(&s.action);
    });
    if let Some(w) = c.works {
        cell(ui, row_h, w, |ui| {
            if s.enabled {
                scope_tag(ui, s);
            } else {
                ui.label(RichText::new("Off").color(t.muted).size(12.0));
            }
        });
    }
    cell(ui, row_h, c.found, |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let tags = (s.estimated as u8 as f32) * GUESS_W + (idle as u8 as f32) * 62.0 + ((!s.enabled && c.works.is_none()) as u8 as f32) * 26.0;
        // The tags follow the text; a long source is cut so the tags keep their room.
        ui.scope(|ui| {
            ui.set_max_width((c.found - tags).max(40.0));
            ui.add(egui::Label::new(RichText::new(s.origin.label()).color(t.muted).size(small)).truncate().selectable(false)).on_hover_text(&s.detail);
        });
        if s.estimated {
            guess_tag(ui);
        }
        if idle {
            ui.label(RichText::new("not open").color(t.muted).size(11.5)).on_hover_text("This app is not open now. Its keys only work while it runs.");
        }
        if !s.enabled && c.works.is_none() {
            ui.label(RichText::new("off").color(t.muted).size(11.5));
        }
    });
    // Where to change it: on the row under the pointer and the selected row, so the list stays quiet.
    cell(ui, row_h, c.fix, |ui| {
        if hovered || selected {
            let fix = crate::fix::fix_for(s);
            if fix.action != crate::fix::FixAction::Nothing
                && ui.link(RichText::new(format!("{} ›", fix.label)).color(t.accent).size(12.0)).on_hover_text(&fix.how).clicked()
            {
                crate::fix::run(&fix.action);
            }
        }
    });
    if resp.clicked() {
        *sel = Some(i);
    }
    if resp.double_clicked() {
        act = Some(RowAct::ShowApp(s.owner()));
    }
    resp.context_menu(|ui| {
        *sel = Some(i);
        if let Some(a) = row_actions(ui, s, true, false) {
            act = Some(a);
        }
    });
    act
}

/// One entry of the By app list.
struct AppEntry {
    id: String,
    name: String,
    count: usize,
    open: bool,
    presses: u32,
}

impl AtlasApp {
    /// Close Peek and log why, so a close that surprises the person can be traced in hotkey.log.
    fn close_panel(&mut self, why: &str) {
        let origin = self.panel.as_ref().and_then(|p| p.focus_origin.clone());
        self.dismiss_panel(why);
        if self.screenshot.is_none() {
            if let Some(origin) = origin.filter(crate::hotkey::should_return_focus) {
                crate::hotkey::give_focus_back(&origin);
            }
        }
    }

    fn dismiss_panel(&mut self, why: &str) {
        if self.panel.is_some() {
            crate::hotkey::log(&format!("peek closed: {why}"));
        }
        self.set_panel(None);
    }

    fn set_panel(&mut self, mut p: Option<crate::panel::Panel>) {
        if let Some(new) = p.as_mut() {
            new.focus_origin = match &self.panel {
                Some(old) => old.focus_origin.clone(),
                None => crate::hotkey::front_app(),
            };
        }
        if let Some(old) = self.panel.as_ref() {
            old.save_place();
        }
        self.panel_open.store(p.is_some(), std::sync::atomic::Ordering::Relaxed);
        self.panel = p;
    }

    /// Messages from the ⌘ ⌘ watcher. Runs even while the main window is hidden.
    fn poll_hotkey(&mut self, ctx: &egui::Context) {
        let msgs: Vec<crate::hotkey::HotkeyMsg> = self.hk_rx.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        for m in msgs {
            match m {
                crate::hotkey::HotkeyMsg::Toggle(front) => {
                    if self.panel.is_some() {
                        self.close_panel("toggle");
                    } else if let Some(app) = front {
                        let mut p = crate::panel::Panel::open(app, &self.data.shortcuts, crate::ax::is_trusted(), &self.not_running);
                        // A quick double press: Peek takes the keyboard, so you can type to filter.
                        p.wants_focus = true;
                        self.set_panel(Some(p));
                    }
                }
                crate::hotkey::HotkeyMsg::ShowHeld(front) => {
                    if let Some(app) = front {
                        let mut p = crate::panel::Panel::open(app, &self.data.shortcuts, crate::ax::is_trusted(), &self.not_running);
                        p.held = true;
                        self.set_panel(Some(p));
                    }
                }
                crate::hotkey::HotkeyMsg::ReleasedAll => {
                    if self.panel.as_ref().is_some_and(|p| p.held && !p.pinned && p.flash.is_none()) {
                        self.close_panel("released all keys");
                    }
                }
                crate::hotkey::HotkeyMsg::KeyPressed(keys) => {
                    // Keys typed inside Peek itself (the filter, arrows) are handled there.
                    let focused = ctx.input(|i| i.raw.viewports.get(&panel_viewport()).and_then(|v| v.focused)).unwrap_or(false);
                    let Some(p) = self.panel.as_mut() else { continue };
                    if focused {
                        continue;
                    }
                    // A shortcut that Peek lists lights up for a moment, so you see what ran.
                    match keys.filter(|k| p.has_row(k)) {
                        Some(k) => p.flash = Some((k, std::time::Instant::now())),
                        None if p.pinned => {}
                        None => self.close_panel("key pressed outside Peek"),
                    }
                }
            }
        }
        // Close after the flash, unless pinned.
        if let Some(p) = self.panel.as_mut() {
            if p.flash.as_ref().is_some_and(|f| f.1.elapsed().as_millis() > crate::panel::FLASH_MS) {
                if p.pinned {
                    p.flash = None;
                } else {
                    self.close_panel("flash ended");
                }
            }
        }
        // A pinned Peek follows the app in front.
        if let Some(p) = self.panel.as_ref().filter(|p| p.pinned) {
            if let Some(front) = crate::hotkey::last_other_app().filter(|f| f.bundle != p.app.bundle) {
                // Keep the place the person gave it, so the window stays where it is.
                p.save_place();
                let mut np = crate::panel::Panel::open(front, &self.data.shortcuts, crate::ax::is_trusted(), &self.not_running);
                np.focus_origin = crate::hotkey::front_app()
                    .filter(crate::hotkey::should_return_focus)
                    .or_else(|| p.focus_origin.clone());
                np.pinned = true;
                np.first_frame = false;
                self.panel = Some(np);
            }
        }
        if let Some(p) = self.panel.as_mut() {
            p.poll_live();
            // Draw the panel even when the main window is minimised or covered.
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }
    }

    fn show_panel(&mut self, ctx: &egui::Context) {
        let Some(panel) = self.panel.as_mut() else { return };
        let screens = crate::window::work_areas(ctx);
        let anchor = ctx.input(|i| i.viewport().outer_rect.map(|r| r.center())).unwrap_or(screens[0].center());
        let rect = crate::window::peek_rect(panel.placed, panel.size_hint, &screens, anchor);
        if panel.placed.is_some_and(|saved| saved != rect) {
            panel.placed = Some(rect);
        }
        let builder = crate::window::peek_builder(rect, panel.wants_focus);
        let mut outcome = crate::panel::Outcome::Keep;
        let mut open_settings = false;
        ctx.show_viewport_immediate(panel_viewport(), builder, |ui, _class| {
            open_settings = ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma));
            // Held open: do not take focus from the app you are working in.
            // A quick double press takes focus, so you can type to filter.
            if panel.first_frame && panel.wants_focus {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            outcome = panel.draw(ui);
            if ui.input(|i| i.viewport().close_requested()) {
                crate::hotkey::log("peek: window close requested");
                outcome = crate::panel::Outcome::Close;
            }
        });
        match outcome {
            crate::panel::Outcome::Keep => {}
            crate::panel::Outcome::Close => self.close_panel("closed in Peek (Esc, Close button, or window)"),
            crate::panel::Outcome::Run(s) => {
                let app = panel.app.clone();
                let pinned = panel.pinned;
                if !pinned {
                    self.dismiss_panel("ran a shortcut");
                }
                crate::hotkey::give_focus_back(&app);
                run_later(app, Some(s), None);
            }
            crate::panel::Outcome::Pass(t) => {
                let app = panel.app.clone();
                self.dismiss_panel("passed keys to the app");
                crate::hotkey::give_focus_back(&app);
                run_later(app, None, Some(t));
            }
        }
        if open_settings {
            self.open_settings(ctx);
        }
    }
}

impl AtlasApp {
    fn main_ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let t = th();
        // Background results.
        let msgs: Vec<Msg> = self.rx.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        for m in msgs {
            match m {
                Msg::Quick(q) => {
                    self.quick = q;
                    self.loaded = true;
                    self.deep = None;
                    self.apply();
                }
                Msg::Deep(d) => {
                    self.deep = Some(d);
                    self.apply();
                    self.scanning = false;
                    self.rx = None;
                }
            }
        }
        if self.scanning {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }

        if self.show_welcome {
            let mut start = false;
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(t.bg).inner_margin(egui::Margin::symmetric(20, 14)))
                .show(ui, |ui| start = crate::welcome::show(ui, &mut self.welcome_count, self.logo.as_ref()));
            if start {
                crate::settings::update(|s| s.welcomed = true);
                crate::usage::set_paused(!self.welcome_count);
                self.show_welcome = false;
            }
            return;
        }

        // ⌘F: the search field of this tab. ⌘ + and ⌘ −: the text size of the lists (shared with Peek).
        if !self.recording && matches!(self.tab, Tab::Conflicts | Tab::All | Tab::Lookup | Tab::Apps) {
            use egui::{Key, Modifiers};
            let lists = matches!(self.tab, Tab::All | Tab::Lookup | Tab::Apps);
            ctx.input_mut(|i| {
                if i.consume_key(Modifiers::COMMAND, Key::F) {
                    self.focus_search = true;
                }
                if lists {
                    let font = crate::settings::get().panel_font;
                    let (lo, hi) = crate::settings::PANEL_FONT_RANGE;
                    if i.consume_key(Modifiers::COMMAND, Key::Equals) || i.consume_key(Modifiers::COMMAND, Key::Plus) || i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Equals) {
                        crate::settings::update(|s| s.panel_font = (font + 1.0).min(hi));
                    }
                    if i.consume_key(Modifiers::COMMAND, Key::Minus) {
                        crate::settings::update(|s| s.panel_font = (font - 1.0).max(lo));
                    }
                    if i.consume_key(Modifiers::COMMAND, Key::Num0) {
                        crate::settings::update(|s| s.panel_font = crate::settings::DEFAULT_PANEL_FONT);
                    }
                }
            });
        }
        egui::Panel::top("header")
            .frame(egui::Frame::new().fill(t.surface).inner_margin(egui::Margin { left: 20, right: 20, top: 0, bottom: 0 }))
            .show(ui, |ui| self.header(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(t.bg).inner_margin(egui::Margin::symmetric(20, 14)))
            .show(ui, |ui| {
                ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
                    node.set_role(egui::Role::Group);
                    node.set_label("Shortcut content");
                });
                if !self.loaded {
                    ui.add_space(90.0);
                    ui.vertical_centered(|ui| {
                        ui.spinner();
                        ui.add_space(6.0);
                        ui.label(medium("Reading shortcuts…").size(16.0).color(t.text2));
                        ui.label(RichText::new("macOS, app settings, config files and app menus").color(t.muted));
                    });
                    return;
                }
                match self.tab {
                    Tab::Conflicts => self.conflicts_tab(ui),
                    Tab::Lookup => self.lookup_tab(ui),
                    Tab::All => self.all_tab(ui),
                    Tab::Apps => self.apps_tab(ui),
                    Tab::Sources => self.sources_tab(ui),
                    Tab::Usage => {
                        let list = &self.data.shortcuts;
                        self.use_view.show(ui, list, &self.data.apps, self.scan_gen);
                        // A press counted in the background shows without a click.
                        ctx.request_repaint_after(std::time::Duration::from_secs(1));
                    }
                    Tab::Keyboard => {
                        let conflicts = &self.conflicts;
                        let list = &self.data.shortcuts;
                        // The press counts are read only for the Use colors.
                        let used = if self.keyboard.heat { self.use_view.report(list, self.scan_gen).0 } else { Default::default() };
                        self.keyboard.show(ui, list, conflicts, &mut self.use_view, &used);
                    }
                }
            });
    }
}

impl AtlasApp {
    fn open_settings(&mut self, ctx: &egui::Context) {
        self.settings_open = true;
        ctx.send_viewport_cmd_to(settings_viewport(), egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }

    fn show_settings(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let builder = egui::ViewportBuilder::default()
            .with_title("Keyclash Settings")
            .with_inner_size([680.0, 760.0])
            .with_min_inner_size([560.0, 420.0])
            .with_resizable(true);
        let mut change = crate::welcome::Change::Nothing;
        let mut close = false;
        ctx.show_viewport_immediate(settings_viewport(), builder, |ui, _class| {
            theme::sync(ui.ctx());
            close = ui.input_mut(|i| {
                i.viewport().close_requested()
                    || i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                    || i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
            });
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(th().bg).inner_margin(egui::Margin::symmetric(20, 14)))
                .show(ui, |ui| change = crate::welcome::settings_content(ui, self.screenshot.is_none()));
        });
        match change {
            crate::welcome::Change::ScanAgain if !self.scanning => self.start_scan(ctx),
            crate::welcome::Change::ShowWelcome => {
                self.welcome_count = crate::settings::get().count_keys;
                self.show_welcome = true;
                close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            _ => {}
        }
        if close {
            self.settings_open = false;
        }
    }
}

fn settings_viewport() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("settings")
}

fn panel_viewport() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("shortcuts-panel")
}

/// Run a shortcut from Peek in its app, a moment after focus goes back to that app:
/// press the menu item when it is one, else send its keys.
fn run_later(app: crate::hotkey::FrontApp, s: Option<Shortcut>, keys: Option<Trigger>) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        if let Some(s) = s {
            let own = s.bundle.as_deref() == Some(app.bundle.as_str());
            let pressed = own && crate::panel::menu_path(&s).is_some_and(|path| crate::ax::press_menu_item(app.pid, &path));
            if !pressed {
                // A system-wide hotkey of another tool goes through the system; the app's own keys go to the app.
                let sent = crate::hotkey::post_keys(&s.trigger, own.then_some(app.pid));
                crate::hotkey::log(&format!("peek run {:?} in {}: menu {pressed}, keys {sent}", s.action, app.bundle));
            }
        } else if let Some(t) = keys {
            crate::hotkey::post_keys(&t, Some(app.pid));
        }
    });
}

impl eframe::App for AtlasApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        input.events.retain(|event| {
            if matches!(event, egui::Event::Key { key: egui::Key::Comma, pressed: true, modifiers, .. } if modifiers.matches_logically(egui::Modifiers::COMMAND)) {
                self.open_settings(ctx);
                return false;
            }
            true
        });
        if input.viewport_id == egui::ViewportId::ROOT && self.tab == Tab::Keyboard && self.panel.is_none() && !self.show_welcome {
            self.keyboard.handle_input(input, crate::hotkey::current_mods());
        }
    }

    /// Counts that are still only in memory go to the file.
    fn on_exit(&mut self) {
        crate::usage::flush();
    }

    /// `--screenshot`: runs every frame, and also while the window is covered or hidden,
    /// so the picture is taken even when nobody can see the window.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::sync(ctx);
        self.follow_layout(ctx);
        self.poll_learn();
        self.poll_hotkey(ctx);
        let Some(path) = self.screenshot.clone() else { return };
        self.frames += 1;
        // Keep drawing even if the window is covered, minimised or on another screen.
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
        ctx.request_repaint();
        let waited = self.started.elapsed().as_secs_f32();
        if !self.shot_asked && (self.frames >= 120 || waited >= 15.0) {
            self.shot_asked = true;
            let slot = self.shot.clone();
            let c2 = ctx.clone();
            let shot = egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                *slot.lock().unwrap() = Some(img);
                c2.request_repaint();
            }));
            ctx.send_viewport_cmd(shot);
        }
        // Ask again if no picture came back.
        if self.shot_asked && waited >= 30.0 && (self.frames % 30 == 0) {
            self.shot_asked = false;
        }
        let taken = self.shot.lock().unwrap().take();
        if let Some(img) = taken {
            save_png(&img, &path);
            std::process::exit(0);
        }
        if waited >= 80.0 {
            eprintln!("no window picture after 80 s; giving up");
            std::process::exit(1);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        theme::sync(&ctx);
        // `--screenshot … --panel <bundle id>`: draw the panel in the main window to photograph it.
        if self.screenshot.is_some() {
            if self.settings_open {
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(th().bg).inner_margin(egui::Margin::symmetric(20, 14)))
                    .show(ui, |ui| { crate::welcome::settings_content(ui, false); });
                return;
            }
            if let Some(p) = self.panel.as_mut() {
                p.draw(ui);
                return;
            }
        }
        self.main_ui(ui, frame);
        self.show_settings(&ctx);
        // Draw Peek last. Both windows share one texture of letter shapes. Letters first drawn in a
        // pass go to the GPU with the next window that paints. When Peek paints last, it sends every
        // new letter of this frame. Before, the main window kept some of them, and while it was hidden
        // it painted them late, over newer letters: Peek then showed words with letters missing.
        self.show_panel(&ctx);
    }

}

fn save_png(img: &egui::ColorImage, path: &std::path::Path) {
    let [w, h] = img.size;
    let mut buf = Vec::with_capacity(w * h * 4);
    for p in &img.pixels {
        buf.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
    }
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    match image::RgbaImage::from_raw(w as u32, h as u32, buf) {
        Some(im) => {
            if let Err(e) = im.save(path) {
                eprintln!("could not save screenshot: {e}");
            }
        }
        None => eprintln!("screenshot had an unexpected size"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sc(app: &str, action: &str, t: Trigger, scope: Scope) -> Shortcut {
        Shortcut::new(app, action, t, scope, Origin::LiveMenu).bundle(Some(&format!("test.{}", app.to_lowercase())))
    }

    #[test]
    fn keys_sort_in_macos_modifier_order_with_mode_keys_last() {
        let all = [
            sc("AeroSpace", "resize", Trigger::combo(0, "-"), Scope::Mode),
            sc("A", "both", Trigger::combo(CMD | SHIFT, "T"), Scope::App),
            sc("A", "cmd", Trigger::combo(CMD, "T"), Scope::App),
            sc("A", "ctrl", Trigger::combo(CTRL, "T"), Scope::App),
            sc("macOS", "F11", Trigger::combo(0, "F11"), Scope::Global),
            sc("A", "opt", Trigger::combo(OPT, "B"), Scope::App),
        ];
        let mut rows: Vec<&Shortcut> = all.iter().collect();
        sort_rows(&mut rows, SortCol::Keys, false);
        let order: Vec<&str> = rows.iter().map(|s| s.action.as_str()).collect();
        assert_eq!(order, ["F11", "ctrl", "opt", "cmd", "both", "resize"]);
        // A click on App sorts by app; a second click turns it around.
        sort_rows(&mut rows, SortCol::App, true);
        assert_eq!(rows[0].app, "macOS");
    }

    #[test]
    fn system_wide_leaves_out_mode_keys() {
        let mode = sc("AeroSpace", "resize", Trigger::combo(0, "-"), Scope::Mode);
        let global = sc("AeroSpace", "focus", Trigger::combo(OPT, "H"), Scope::Global);
        assert!(!ScopeFilter::Global.keeps(&mode));
        assert!(ScopeFilter::Global.keeps(&global));
        assert!(ScopeFilter::Mode.keeps(&mode));
        assert!(!ScopeFilter::App.keeps(&mode));
    }

    #[test]
    fn near_matches_count_apps_per_modifier_set() {
        let all = [
            sc("A", "new tab", Trigger::combo(CMD, "T"), Scope::App),
            sc("B", "new tab", Trigger::combo(CMD, "T"), Scope::App),
            sc("C", "x", Trigger::combo(OPT | CMD, "T"), Scope::App),
            sc("D", "reopen", Trigger::combo(CMD | SHIFT, "T"), Scope::App),
        ];
        let near = near_matches(&Trigger::combo(CMD | SHIFT, "T"), &all);
        assert_eq!(near, vec![(Trigger::combo(CMD, "T"), 2), (Trigger::combo(OPT | CMD, "T"), 1)]);
        assert!(near_matches(&Trigger::ModTap { mods: OPT, taps: 2 }, &all).is_empty());
    }

    #[test]
    fn only_my_apps_keeps_tools_without_an_app() {
        let mine: HashSet<String> = ["test.a".to_string()].into_iter().collect();
        assert!(is_mine(&sc("A", "x", Trigger::combo(CMD, "T"), Scope::App), &mine));
        assert!(!is_mine(&sc("B", "x", Trigger::combo(CMD, "T"), Scope::App), &mine));
        assert!(is_mine(&Shortcut::new("skhd", "x", Trigger::combo(CMD, "T"), Scope::Global, Origin::ConfigFile), &mine));
    }

    #[test]
    fn export_escapes_cells() {
        let s = sc("A|B", "Say \"hi\"", Trigger::combo(CMD, "T"), Scope::App);
        let rows = vec![&s];
        assert!(list_markdown(&rows).contains("| ⌘T | A\\|B | Say \"hi\" | In app |"));
        let csv = list_csv(&rows);
        assert!(csv.lines().nth(1).unwrap().starts_with("\"⌘T\",\"cmd t\",\"A|B\",\"Say \"\"hi\"\"\",In app,"));
    }

    #[test]
    fn list_columns_give_the_action_the_rest() {
        let c = ListCols::fit(1150.0, true, true, 13.5);
        // The default window leaves the action about 360 points.
        assert!(c.action > 340.0, "{}", c.action);
        let no_works = ListCols::fit(1150.0, false, false, 13.5);
        assert!(no_works.action > c.action + 250.0);
        assert_eq!(c.names().len(), 6);
        assert_eq!(no_works.names().len(), 4);
    }
}
