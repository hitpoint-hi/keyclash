//! Peek: press ⌘ twice in any app and a floating box lists that app's keys, grouped by menu
//! in columns (at most 4).
//! Holding modifier keys highlights the shortcuts that use them.
//! A quick double press opens it with keyboard focus: type to filter, arrows to move, Return to run.
//! A double press that is held shows it only while the key is held, without focus.
//! The same grouped view is used for one app in the Apps tab.

use crate::hotkey::FrontApp;
use crate::keys::{mod_symbols, trigger_text, trigger_words};
use crate::model::*;
use crate::theme::{medium, mix, overline, segmented, semi, th};
use crate::ui::keycaps;
use eframe::egui::{self, RichText, Stroke};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::time::Instant;

/// The user-facing name of the panel.
pub const NAME: &str = "Peek";

/// The most columns Peek uses.
pub const COLUMNS: usize = 4;
/// The widest Peek gets: 0.8 of the screen.
pub const WIDTH_SHARE: f32 = 0.8;
/// The tallest Peek gets: 0.6875 of the screen.
pub const HEIGHT_SHARE: f32 = 0.6875;
/// A column gets at least this many lines before Peek adds the next column.
const LINES_PER_COLUMN: usize = 10;
/// How long a pressed shortcut stays lit before Peek closes.
pub const FLASH_MS: u128 = 450;

/// How held modifier keys pick the shortcuts to highlight.
/// In the default highlight mode an item matches when its modifier
/// flags equal the held flags (|item - held| < 0x200 after `item & held != 0`): "fully matched".
/// "Any key" matches any held modifier key.
/// "Includes" matches all held keys, and allows more.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HighlightMode {
    Exact = 0,
    Includes = 1,
    Any = 2,
}

impl HighlightMode {
    pub fn name(self) -> &'static str {
        match self {
            HighlightMode::Exact => "Exact keys",
            HighlightMode::Includes => "Includes keys",
            HighlightMode::Any => "Any key",
        }
    }

    fn next(self) -> HighlightMode {
        match self {
            HighlightMode::Exact => HighlightMode::Includes,
            HighlightMode::Includes => HighlightMode::Any,
            HighlightMode::Any => HighlightMode::Exact,
        }
    }
}

static MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(HighlightMode::Exact as u8);
static MODE_LOADED: std::sync::Once = std::sync::Once::new();

pub fn mode() -> HighlightMode {
    // The saved choice, read once. Tests do not read the file.
    #[cfg(not(test))]
    MODE_LOADED.call_once(|| MODE.store(crate::settings::get().highlight_mode.min(2), std::sync::atomic::Ordering::Relaxed));
    match MODE.load(std::sync::atomic::Ordering::Relaxed) {
        1 => HighlightMode::Includes,
        2 => HighlightMode::Any,
        _ => HighlightMode::Exact,
    }
}

/// Change the mode and keep the choice.
pub fn set_mode(m: HighlightMode) {
    MODE_LOADED.call_once(|| {});
    MODE.store(m as u8, std::sync::atomic::Ordering::Relaxed);
    #[cfg(not(test))]
    crate::settings::update(|s| s.highlight_mode = m as u8);
}

/// Flags closer than this count as the same modifier set.
pub const HIGHLIGHT_TOLERANCE: i64 = 512;

pub fn matches_held(t: &Trigger, held: u8) -> bool {
    let m = t.mods();
    if held == 0 || m & held == 0 {
        return false;
    }
    match mode() {
        HighlightMode::Exact => {
            let (item, now) = (crate::keys::mods_to_nsevent(m), crate::keys::mods_to_nsevent(held));
            (item - now).abs() < HIGHLIGHT_TOLERANCE
        }
        HighlightMode::Includes => m & held == held,
        HighlightMode::Any => true,
    }
}

/// The three choices that pick the highlight mode.
pub fn mode_picker(ui: &mut egui::Ui) {
    let t = th();
    let mut m = mode();
    let choices = [
        (HighlightMode::Exact, "Exact keys", "Only shortcuts that use exactly the keys you hold"),
        (HighlightMode::Includes, "Includes keys", "Shortcuts that use all the keys you hold, plus maybe more"),
        (HighlightMode::Any, "Any key", "Shortcuts that use any of the keys you hold"),
    ];
    // In a right-to-left row the label is added last, so it still reads first.
    let rtl = ui.layout().main_dir() == egui::Direction::RightToLeft;
    if !rtl {
        ui.label(RichText::new("Highlight").color(t.muted).size(12.0));
    }
    if segmented(ui, &mut m, &choices) {
        set_mode(m);
    }
    if rtl {
        ui.label(RichText::new("Highlight").color(t.muted).size(12.0));
    }
}

/// "1 match", "3 matches".
pub fn matches_text(n: usize) -> String {
    format!("{n} {}", if n == 1 { "match" } else { "matches" })
}

/// The modifier keys that are not held yet, for the "Add … to narrow" hint.
pub fn not_held(held: u8) -> String {
    [(CTRL, "⌃"), (OPT, "⌥"), (SHIFT, "⇧"), (CMD, "⌘")].iter().filter(|(b, _)| held & b == 0).map(|(_, s)| *s).collect::<Vec<_>>().join(" ")
}

/// All of one app's shortcuts (menus, settings, config files), turned on.
pub fn app_items(all: &[Shortcut], bundle: &str) -> Vec<Shortcut> {
    all.iter()
        .filter(|s| s.enabled && s.scope != Scope::Mode && s.bundle.as_deref() == Some(bundle))
        .cloned()
        .collect()
}

/// Every shortcut of one owner (an app id, or the name of a tool without an app), turned on,
/// with the keys of its modes. The By app tab lists these, and counts the same rows.
pub fn owner_items(all: &[Shortcut], owner: &str) -> Vec<Shortcut> {
    all.iter().filter(|s| s.enabled && s.owner() == owner).cloned().collect()
}

/// The mode of a mode key, from its config detail: "… · [mode.service.binding] r" gives "Service mode".
fn mode_name(s: &Shortcut) -> String {
    s.detail
        .split("[mode.")
        .nth(1)
        .and_then(|r| r.split('.').next())
        .filter(|m| !m.is_empty())
        .map(|m| {
            let mut c = m.chars();
            format!("{}{} mode", c.next().map(|f| f.to_uppercase().to_string()).unwrap_or_default(), c.as_str())
        })
        .unwrap_or_else(|| "In a mode".into())
}

/// System-wide hotkeys of other tools that work while this app is in front ("Also active here").
/// Tools that are not open are left out. macOS keys that the app's own menus repeat are left out.
pub fn also_active(all: &[Shortcut], bundle: &str, not_running: &HashSet<String>) -> Vec<Shortcut> {
    let mut out: Vec<Shortcut> = all
        .iter()
        .filter(|s| s.enabled && s.scope == Scope::Global && s.bundle.as_deref() != Some(bundle))
        .filter(|s| !s.bundle.as_ref().is_some_and(|b| not_running.contains(b)))
        .filter(|s| !(s.bundle.as_deref() == Some(crate::sources::MACOS) && crate::conflicts::mirrored_by_menus(&s.trigger)))
        .cloned()
        .collect();
    // Other tools first, macOS last; by tool, then by keys.
    out.sort_by(|a, b| {
        let mac = |s: &Shortcut| s.bundle.as_deref() == Some(crate::sources::MACOS);
        mac(a).cmp(&mac(b)).then_with(|| a.app.to_lowercase().cmp(&b.app.to_lowercase())).then_with(|| a.trigger.cmp(&b.trigger))
    });
    out
}

/// A system-wide shortcut of another tool on the same keys.
pub struct Taker {
    /// The tool's name, for the mark next to the action.
    pub app: String,
    /// "Tool: action", for the hover text.
    pub text: String,
    /// The action name when the taker is macOS itself.
    macos_action: Option<String>,
}

impl Taker {
    /// False when macOS and the app's own menu item do the same thing (Minimize, Fill, Center…).
    /// This is the rule the Conflicts list uses.
    pub fn blocks(&self, t: &Trigger, action: &str) -> bool {
        match &self.macos_action {
            Some(a) => !(crate::conflicts::mirrored_by_menus(t) || *a == action.to_lowercase()),
            None => true,
        }
    }
}

/// System-wide shortcuts of other tools, by trigger: they take the key first.
pub fn taken_by(all: &[Shortcut], bundle: &str) -> HashMap<Trigger, Taker> {
    let mut m = HashMap::new();
    for s in all.iter().filter(|s| s.enabled && s.scope == Scope::Global && s.bundle.as_deref() != Some(bundle)) {
        m.entry(s.trigger.clone()).or_insert_with(|| Taker {
            app: s.app.clone(),
            text: format!("{}: {}", s.app, s.action),
            macos_action: (s.bundle.as_deref() == Some(crate::sources::MACOS)).then(|| s.action.to_lowercase()),
        });
    }
    m
}

fn is_menu(s: &Shortcut) -> bool {
    matches!(s.origin, Origin::LiveMenu | Origin::RememberedMenu | Origin::MenuFile)
}

/// The menu path of a menu item: "Edit › Find › Find Next" gives ["Edit", "Find", "Find Next"].
pub fn menu_path(s: &Shortcut) -> Option<Vec<String>> {
    if !is_menu(s) {
        return None;
    }
    let body = s.detail.split(" · ").next().unwrap_or("").trim();
    let parts: Vec<String> = body.split(" › ").map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
    (parts.len() >= 2).then_some(parts)
}

/// Menu name for grouping, and the submenu inside it: "Edit › Find › Find Next" gives ("Edit", Some("Find")).
fn group_of(s: &Shortcut) -> (String, Option<String>) {
    if is_menu(s) {
        let body = s.detail.split(" · ").next().unwrap_or("").trim();
        let parts: Vec<&str> = body.split(" › ").map(str::trim).collect();
        let first = parts.first().copied().unwrap_or("");
        let group = if first.is_empty() || first == s.action || parts.len() < 2 { "Menu".to_string() } else { first.to_string() };
        let sub = (parts.len() >= 3).then(|| parts[1..parts.len() - 1].join(" › "));
        return (group, sub);
    }
    if s.scope == Scope::Global {
        return ("System-wide".into(), None);
    }
    if s.scope == Scope::Mode {
        return (mode_name(s), None);
    }
    (s.origin.label().to_string(), None)
}

pub const STARRED: &str = "★ Starred";
pub const ALSO_ACTIVE: &str = "Also active here";
pub const HIDDEN: &str = "Hidden";

/// One row and where it goes.
pub struct Entry<'a> {
    pub s: &'a Shortcut,
    group: String,
    sub: Option<String>,
    /// A system-wide hotkey of another tool ("Also active here").
    pub other_tool: bool,
}

/// One line of a column.
#[derive(Debug, PartialEq)]
pub enum Line {
    Head(String),
    /// A group that goes on from the column before. Empty when the list has one group:
    /// then only a space keeps the rows level with the first column.
    Cont(String),
    Sub(String),
    /// An index into the entries.
    Item(usize),
}

/// The rows to show, in groups: starred first, then system-wide, the menus, other tools, hidden rows.
/// `query` keeps the rows whose action, menu or keys contain each of its words.
pub fn entries<'a>(items: &'a [Shortcut], extra: &'a [Shortcut], query: &str, show_hidden: bool) -> Vec<Entry<'a>> {
    let words: Vec<String> = query.to_lowercase().split_whitespace().map(String::from).collect();
    let keep = |s: &Shortcut, group: &str| {
        if words.is_empty() {
            return true;
        }
        let hay = format!("{} {} {} {} {}", s.action, group, s.detail, trigger_words(&s.trigger), trigger_text(&s.trigger)).to_lowercase();
        let hay = if s.app.is_empty() { hay } else { format!("{hay} {}", s.app.to_lowercase()) };
        words.iter().all(|w| hay.contains(w.as_str()))
    };
    let mut out: Vec<Entry> = Vec::new();
    for s in items {
        let hidden = crate::favorites::is_hidden(s);
        if hidden && !show_hidden {
            continue;
        }
        let (group, sub) = group_of(s);
        if !keep(s, &group) {
            continue;
        }
        // Starred first: shown at the start of the list.
        let (group, sub) = if hidden {
            (HIDDEN.to_string(), None)
        } else if crate::favorites::is_starred(s) {
            (STARRED.to_string(), None)
        } else {
            (group, sub)
        };
        out.push(Entry { s, group, sub, other_tool: false });
    }
    for s in extra {
        if keep(s, ALSO_ACTIVE) && !crate::favorites::is_hidden(s) {
            out.push(Entry { s, group: ALSO_ACTIVE.to_string(), sub: None, other_tool: true });
        }
    }
    // Groups keep their first-seen order inside each rank; rows keep theirs inside each group.
    let rank = |g: &str| match g {
        STARRED => 0,
        "System-wide" => 1,
        ALSO_ACTIVE => 3,
        HIDDEN => 4,
        _ => 2,
    };
    let mut first_seen: HashMap<String, usize> = HashMap::new();
    for (i, e) in out.iter().enumerate() {
        first_seen.entry(e.group.clone()).or_insert(i);
    }
    let mut idx: Vec<usize> = (0..out.len()).collect();
    idx.sort_by_key(|&i| (rank(&out[i].group), first_seen[&out[i].group], i));
    let mut slots: Vec<Option<Entry>> = out.into_iter().map(Some).collect();
    idx.into_iter().filter_map(|i| slots[i].take()).collect()
}

/// How many columns `n` entries in `groups` groups need, from 1 to `max`.
pub fn columns_for(n: usize, groups: usize, max: usize) -> usize {
    (n + groups * 2).div_ceil(LINES_PER_COLUMN).clamp(1, max.max(1))
}

/// Flow the groups down the columns like text, so every column gets the same share.
/// A long group continues at the top of the next column.
pub fn layout(entries: &[Entry], columns: usize) -> Vec<Vec<Line>> {
    let columns = columns.max(1);
    let groups = entries.iter().map(|e| &e.group).collect::<HashSet<_>>().len();
    let one_group = groups == 1;
    let subs = entries.windows(2).filter(|w| w[1].sub.is_some() && w[1].sub != w[0].sub).count();
    let total: usize = entries.len() + groups * 2 + subs;
    let share = total.div_ceil(columns).max(1);
    let mut cols: Vec<Vec<Line>> = (0..columns).map(|_| Vec::new()).collect();
    let (mut c, mut used) = (0, 0);
    let mut i = 0;
    while i < entries.len() {
        let g = &entries[i].group;
        let end = (i..entries.len()).find(|&j| &entries[j].group != g).unwrap_or(entries.len());
        // Do not start a group at the very end of a column.
        if used > 0 && used + 3 > share && c + 1 < columns {
            c += 1;
            used = 0;
        }
        cols[c].push(Line::Head(g.clone()));
        used += 1;
        let mut sub: Option<&String> = None;
        for (k, j) in (i..end).enumerate() {
            let left = end - j;
            // Break only when two or more items move, so no item stands alone.
            if used >= share && c + 1 < columns && k >= 2 && left >= 2 {
                c += 1;
                used = 1;
                cols[c].push(Line::Cont(if one_group { String::new() } else { g.clone() }));
                sub = None;
            }
            if entries[j].sub.as_ref() != sub {
                sub = entries[j].sub.as_ref();
                if let Some(s) = sub {
                    cols[c].push(Line::Sub(s.clone()));
                    used += 1;
                }
            }
            cols[c].push(Line::Item(j));
            used += 1;
        }
        used += 1;
        i = end;
    }
    cols.retain(|c| !c.is_empty());
    if cols.is_empty() {
        cols.push(Vec::new());
    }
    cols
}

/// Row height for a text size.
fn row_height(font: f32) -> f32 {
    (font + 9.0).round()
}

/// The height of the tallest column, in points.
pub fn columns_height(cols: &[Vec<Line>], font: f32) -> f32 {
    cols.iter()
        .map(|col| {
            col.iter()
                .enumerate()
                .map(|(i, l)| match l {
                    Line::Head(_) | Line::Cont(_) => (if i > 0 { 12.0 } else { 0.0 }) + 17.0,
                    Line::Sub(_) => 19.0,
                    Line::Item(_) => row_height(font) + 1.0,
                })
                .sum::<f32>()
        })
        .fold(0.0, f32::max)
}

/// What happened in the rows this frame.
pub enum RowEvent {
    /// Run this entry (a click or Return).
    Run(usize),
    Select(usize),
}

/// Everything the rows need to draw.
pub struct RowsView<'a> {
    pub entries: &'a [Entry<'a>],
    pub cols: &'a [Vec<Line>],
    pub taken: &'a HashMap<Trigger, Taker>,
    pub held: u8,
    pub uses: &'a HashMap<Trigger, u32>,
    pub selected: Option<usize>,
    /// Scroll the selected row into view (it changed this frame).
    pub reveal: bool,
    pub flash: Option<&'a Trigger>,
    pub font: f32,
    /// macOS "Increase contrast" is on: dimmed rows stay darker.
    pub strong: bool,
    /// Rows can be run with a click (Peek). In the Apps tab a click selects.
    pub runnable: bool,
}

/// Shortcuts grouped by menu, in columns, with held-key highlighting.
pub fn draw_rows(ui: &mut egui::Ui, v: &RowsView) -> Option<RowEvent> {
    let t = th();
    let mut event = None;
    let row_h = row_height(v.font);
    let cap_size = (v.font - 2.0).max(9.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        ui.columns(v.cols.len().max(1), |cs| {
            for (ci, col) in v.cols.iter().enumerate() {
                let ui = &mut cs[ci];
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 1.0);
                for (li, line) in col.iter().enumerate() {
                    let i = match line {
                        Line::Head(g) => {
                            if li > 0 {
                                ui.add_space(12.0);
                            }
                            ui.horizontal(|ui| {
                                ui.add_space(5.0);
                                ui.label(overline(g));
                            });
                            ui.add_space(1.0);
                            continue;
                        }
                        Line::Cont(g) => {
                            // "(continued)" is small and muted, so the heading does not read as a new group.
                            ui.horizontal(|ui| {
                                ui.set_min_height(16.0);
                                if !g.is_empty() {
                                    ui.add_space(5.0);
                                    ui.label(overline(g));
                                    ui.label(RichText::new("continued").size(11.0).color(t.muted));
                                }
                            });
                            ui.add_space(1.0);
                            continue;
                        }
                        Line::Sub(s) => {
                            ui.horizontal(|ui| {
                                ui.add_space(5.0);
                                ui.label(RichText::new(format!("{s} ›")).size(11.5).color(t.muted));
                            });
                            continue;
                        }
                        Line::Item(i) => *i,
                    };
                    if let Some(e) = row(ui, v, i, row_h, cap_size) {
                        event = Some(e);
                    }
                }
            }
        });
    });
    event
}

/// One shortcut row: star, action, blocker mark, press count, keys.
fn row(ui: &mut egui::Ui, v: &RowsView, i: usize, row_h: f32, cap_size: f32) -> Option<RowEvent> {
    let t = th();
    let e = &v.entries[i];
    let s = e.s;
    let hit = matches_held(&s.trigger, v.held);
    let flashing = v.flash.is_some_and(|f| *f == s.trigger);
    let selected = v.selected == Some(i);
    let taker = if e.other_tool { None } else { v.taken.get(&s.trigger).filter(|k| k.blocks(&s.trigger, &s.action)) };
    let uses = if e.other_tool { 0 } else { v.uses.get(&s.trigger).copied().unwrap_or(0) };
    let dim = v.held != 0 && !hit && !flashing;
    let color = if dim {
        if v.strong { t.text2 } else { t.muted }
    } else if taker.is_some() {
        t.clash
    } else {
        t.text
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), egui::Sense::click());
    let action = if e.other_tool { format!("{} — {}", s.app, s.action) } else { s.action.clone() };
    let spoken = {
        let mut w = format!("{action}, {}", trigger_words(&s.trigger));
        if let Some(k) = taker {
            w.push_str(&format!(", blocked by {}", k.app));
        }
        if uses > 0 {
            w.push_str(&format!(", pressed {uses} times in 30 days"));
        }
        if s.estimated && !e.other_tool {
            w.push_str(", guess");
        }
        if crate::favorites::is_starred(s) {
            w.push_str(", starred");
        }
        w
    };
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, &spoken));
    if v.reveal && selected {
        ui.scroll_to_rect(rect, None);
    }
    if !ui.is_rect_visible(rect) {
        return None;
    }
    // The frame is always the same size, so a row does not move when it lights up.
    let (fill, line) = if flashing {
        (t.tint(t.accent, 0.38), t.accent)
    } else if hit {
        (t.tint(t.accent, 0.16), t.tint(t.accent, 0.55))
    } else if selected {
        (mix(t.bg, t.text, 0.07), t.line_strong)
    } else if resp.hovered() {
        (mix(t.bg, t.text, 0.04), egui::Color32::TRANSPARENT)
    } else {
        (egui::Color32::TRANSPARENT, egui::Color32::TRANSPARENT)
    };
    ui.painter().rect(rect, 6.0, fill, Stroke::new(if selected { 1.5 } else { 1.0 }, line), egui::StrokeKind::Inside);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(5.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let ui = &mut child;
    ui.spacing_mut().item_spacing.x = 5.0;
    // The star shows on starred rows, and on the row under the pointer or the selected row.
    let starred = crate::favorites::is_starred(s);
    if starred || resp.hovered() || selected || ui.rect_contains_pointer(rect) {
        star_button(ui, s);
    } else {
        ui.add_space(17.0);
    }
    // Room for the key caps (estimated from the key count) and the press count.
    let (caps, key) = crate::keys::trigger_caps(&s.trigger);
    let keys_w = (caps.len() + 1) as f32 * (cap_size * 1.6 + 3.0) + key.chars().count().saturating_sub(1) as f32 * cap_size * 0.6 + 8.0;
    let uses_w = if uses > 0 { 34.0 } else { 0.0 };
    let w = ui.available_width();
    let name_w = (w - keys_w - uses_w).max(w * 0.4);
    ui.allocate_ui_with_layout(egui::vec2(name_w, row_h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_width(name_w);
        ui.spacing_mut().item_spacing.x = 6.0;
        // The blocker's name sits next to the action, so the reason does not need a hover.
        let mark = taker.map(|k| format!("⚠ {}", k.app));
        let mark_w = mark.as_ref().map(|m| (m.chars().count() as f32 * (v.font - 2.0) * 0.58 + 6.0).min(name_w * 0.45)).unwrap_or(0.0);
        // The same "guess" tag as the lists in the main window.
        let guess = s.estimated && !e.other_tool;
        let guess_w = if guess { crate::ui::GUESS_W } else { 0.0 };
        ui.allocate_ui_with_layout(egui::vec2(name_w - mark_w - guess_w, row_h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add(egui::Label::new(RichText::new(&action).color(color).size(v.font)).truncate().selectable(false));
        });
        if guess {
            crate::ui::guess_tag(ui);
        }
        if let Some(m) = mark {
            let c = if dim { color } else { t.clash };
            ui.add(egui::Label::new(RichText::new(m).color(c).size(v.font - 2.0)).truncate().selectable(false));
        }
    });
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // Dim the keys of rows that do not match the held keys.
        if dim {
            ui.set_opacity(if v.strong { 0.75 } else { 0.55 });
        }
        keycaps(ui, &s.trigger, cap_size);
        if uses > 0 {
            ui.label(RichText::new(format!("{}×", crate::theme::num(uses as usize))).size(v.font - 2.5).color(t.muted));
        }
    });
    let mut tip = format!("{} · {}", trigger_text(&s.trigger), if s.detail.is_empty() { s.origin.label() } else { &s.detail });
    if let Some(k) = taker {
        tip.push_str(&format!("\nTaken first by {}. While it runs, this menu item works only from the menu.", k.text));
    }
    if uses > 0 {
        tip.push_str(&format!("\nPressed {uses} times in the last 30 days."));
    }
    if v.runnable {
        tip.push_str("\nClick to run. Right-click for more.");
    }
    let resp = resp.on_hover_text(tip);
    let mut out = None;
    if resp.clicked() {
        out = Some(if v.runnable { RowEvent::Run(i) } else { RowEvent::Select(i) });
    }
    resp.context_menu(|ui| {
        if v.runnable && ui.button("Run").clicked() {
            out = Some(RowEvent::Run(i));
            ui.close();
        }
        if ui.button(if starred { "Unstar" } else { "Star" }).clicked() {
            crate::favorites::toggle(s);
            ui.close();
        }
        let hidden = crate::favorites::is_hidden(s);
        if ui.button(if hidden { "Show again" } else { "Hide" }).clicked() {
            crate::favorites::toggle_hidden(s);
            ui.close();
        }
    });
    out
}

/// The narrowest column of the By app tab, for a text size.
pub fn min_column_width(font: f32) -> f32 {
    360.0 + (font - 13.0) * 18.0
}

/// The fewest columns, up to what the width allows, that show the rows without scrolling.
/// Few rows get wide columns, so their actions are not cut.
pub fn fit_columns(list: &[Entry], width: f32, height: f32, font: f32) -> usize {
    let max = (((width + 18.0) / (min_column_width(font) + 18.0)).floor() as usize).clamp(1, COLUMNS);
    (1..=max).find(|&n| columns_height(&layout(list, n), font) <= height).unwrap_or(max)
}

/// The grouped view of one app, for the By app tab. Hidden rows show in their own group, so they
/// can be shown again. `query` filters like Peek; `uses` gives the press counts.
pub fn draw_groups(ui: &mut egui::Ui, items: &[Shortcut], taken: &HashMap<Trigger, Taker>, held: u8, query: &str, uses: &HashMap<Trigger, u32>) {
    let t = th();
    let list = entries(items, &[], query, true);
    if list.is_empty() {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(medium(format!("No shortcut matches \"{query}\".")).size(16.0).color(t.text2));
            ui.label(RichText::new("Clear the filter to see all shortcuts of this app.").color(t.muted));
        });
        return;
    }
    let font = crate::settings::get().panel_font;
    let n = fit_columns(&list, ui.available_width(), ui.available_height(), font);
    let cols = layout(&list, n);
    let v = RowsView { entries: &list, cols: &cols, taken, held, uses, selected: None, reveal: false, flash: None, font, strong: high_contrast(), runnable: false };
    draw_rows(ui, &v);
}

/// ☆ / ★ toggle for one shortcut.
pub fn star_button(ui: &mut egui::Ui, s: &Shortcut) {
    let t = th();
    let on = crate::favorites::is_starred(s);
    let (txt, col) = if on { ("★", t.star) } else { ("☆", mix(t.card, t.muted, 0.75)) };
    let r = ui.add(egui::Button::new(RichText::new(txt).color(col).size(14.0)).frame(false).small());
    let label = format!("{}: {} in {}", if on { "Unstar" } else { "Star" }, s.action, s.app);
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), on, &label));
    if r.clicked() {
        crate::favorites::toggle(s);
    }
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if on { "Starred: you use this one. Click to unstar." } else { "Star the shortcuts you actually want to use" });
}

// ------------------------------------------------------------ macOS display settings

static HIGH_CONTRAST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// macOS Accessibility › Display › Increase contrast, as last read.
pub fn high_contrast() -> bool {
    HIGH_CONTRAST.load(std::sync::atomic::Ordering::Relaxed)
}

/// Read "Increase contrast" again in the background (it can change while the app runs).
fn refresh_high_contrast() {
    std::thread::spawn(|| {
        let on = std::process::Command::new("/usr/bin/defaults")
            .args(["read", "com.apple.universalaccess", "increaseContrast"])
            .output()
            .ok()
            .is_some_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "1");
        HIGH_CONTRAST.store(on, std::sync::atomic::Ordering::Relaxed);
    });
}

// ------------------------------------------------------------ free keys

/// Letters and digits that no shortcut of this app, and no system-wide hotkey, uses with `held`.
/// Empty when the held keys alone (or with Shift only) would type text.
pub fn free_with(held: u8, used: &HashSet<Trigger>) -> Vec<String> {
    if held & (CMD | CTRL | OPT) == 0 {
        return Vec::new();
    }
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890"
        .chars()
        .map(|c| c.to_string())
        .filter(|k| !used.contains(&Trigger::combo(held, k.clone())))
        .collect()
}

// ------------------------------------------------------------ the panel

/// Where the person left Peek last time, from the settings.
fn saved_place() -> Option<egui::Rect> {
    let s = crate::settings::get();
    let (p, z) = (s.panel_pos?, s.panel_size?);
    Some(egui::Rect::from_min_size(egui::pos2(p[0], p[1]), egui::vec2(z[0].max(480.0), z[1].max(220.0))))
}

/// The shortcut that a key press inside Peek makes, or None. A modifier key by itself is not one:
/// egui reports Shift, Control, Option and Command as key presses too, and holding ⌘ then adding ⇧
/// to highlight must not count as a shortcut.
pub fn shortcut_from_egui(key: egui::Key, modifiers: egui::Modifiers) -> Option<Trigger> {
    use egui::Key;
    if matches!(key, Key::ShiftLeft | Key::ShiftRight | Key::ControlLeft | Key::ControlRight | Key::AltLeft | Key::AltRight | Key::SuperLeft | Key::SuperRight) {
        return None;
    }
    if !(modifiers.mac_cmd || modifiers.command || modifiers.ctrl) {
        return None;
    }
    let mut m = 0;
    if modifiers.mac_cmd || modifiers.command {
        m |= CMD;
    }
    if modifiers.ctrl {
        m |= CTRL;
    }
    if modifiers.alt {
        m |= OPT;
    }
    if modifiers.shift {
        m |= SHIFT;
    }
    let name = match key {
        Key::ArrowLeft => "Left".to_string(),
        Key::ArrowRight => "Right".to_string(),
        Key::ArrowUp => "Up".to_string(),
        Key::ArrowDown => "Down".to_string(),
        Key::Enter => "Return".to_string(),
        Key::Backspace => "Delete".to_string(),
        k => k.symbol_or_name().to_uppercase(),
    };
    Some(Trigger::combo(m, name))
}

/// What the window must do after a frame of the panel.
pub enum Outcome {
    Keep,
    Close,
    /// Close, then run this shortcut in the app.
    Run(Shortcut),
    /// Close, then send these keys to the app (a shortcut pressed inside Peek that it does not list).
    Pass(Trigger),
}

pub struct Panel {
    pub app: FrontApp,
    /// The app that had focus before Peek opened, independent of the shortcuts shown.
    pub focus_origin: Option<FrontApp>,
    pub items: Vec<Shortcut>,
    /// System-wide hotkeys of other open tools.
    pub extra: Vec<Shortcut>,
    pub source: String,
    pub taken_by: HashMap<Trigger, Taker>,
    pub live_rx: Option<mpsc::Receiver<Vec<Shortcut>>>,
    pub first_frame: bool,
    /// Opened by holding ⌘: closes when all modifier keys are released.
    pub held: bool,
    /// Opened by a quick double press or a button: takes keyboard focus for typing.
    pub wants_focus: bool,
    /// Stays open, and follows the app in front, until it is unpinned or closed.
    pub pinned: bool,
    /// The last shortcut pressed while Peek showed, and when.
    pub flash: Option<(Trigger, Instant)>,
    query: String,
    selected: Option<usize>,
    /// The selected shortcut (`favorites::key`). Starring or hiding moves its row, so the
    /// position in `selected` is found again from this key each frame.
    selected_key: Option<String>,
    reveal: bool,
    show_extra: bool,
    show_hidden: bool,
    /// Presses in the last 30 days, by keys, in this app.
    uses: HashMap<Trigger, u32>,
    /// Keys in use in this app or system-wide, for the free keys line.
    used: HashSet<Trigger>,
    /// Width and height for the window, from the content.
    pub size_hint: egui::Vec2,
    /// Columns for the full list.
    columns: usize,
    /// Where the person put Peek before (top-left corner and size), read when it opens.
    /// It stays the same while Peek is open, so the window is not moved back during a drag.
    pub placed: Option<egui::Rect>,
    /// The person moved or resized Peek while it was open.
    moved: bool,
    /// The window's place on screen in the last frame: top-left corner and inner size.
    last_rect: Option<egui::Rect>,
    /// For screenshots: the modifier keys to treat as held.
    pub fake_held: u8,
}

impl Panel {
    pub fn open(app: FrontApp, all: &[Shortcut], ax_trusted: bool, not_running: &HashSet<String>) -> Panel {
        let items = app_items(all, &app.bundle);
        let extra = also_active(all, &app.bundle, not_running);
        let source = items.first().map(|s| s.origin.label().to_string()).unwrap_or_else(|| "nothing read yet".into());
        let taken_by = taken_by(all, &app.bundle);
        if let Some(p) = &app.path {
            crate::icons::register(&app.bundle, p);
        }
        refresh_high_contrast();
        let used = all
            .iter()
            .filter(|s| s.enabled && s.scope != Scope::Mode && (s.scope == Scope::Global || s.bundle.as_deref() == Some(app.bundle.as_str())))
            .map(|s| s.trigger.clone())
            .collect();
        // Presses in the last 30 days (empty when the use counter was never on).
        let mut uses: HashMap<Trigger, u32> = HashMap::new();
        for (t, _, n) in crate::usage::snapshot(crate::usage::Range::Month).1.into_iter().filter(|r| r.1 == app.bundle) {
            *uses.entry(t).or_default() += n;
        }
        // Read the exact menus right now when Accessibility is allowed.
        let live_rx = (ax_trusted && app.pid > 0).then(|| {
            let (tx, rx) = mpsc::channel();
            let a = app.clone();
            std::thread::spawn(move || {
                let limit = crate::sources::BROWSERS.contains(&a.bundle.as_str()).then_some(crate::sources::BROWSER_MENU_LIMIT);
                if let Some(items) = crate::ax::read_app_menus(a.pid, crate::sources::MAX_LIVE_ITEMS, limit) {
                    let name = if a.name.is_empty() { a.bundle.clone() } else { a.name.clone() };
                    let _ = tx.send(crate::sources::live_items_to_shortcuts(&name, &a.bundle, items));
                }
            });
            rx
        });
        let mut p = Panel {
            app,
            focus_origin: None,
            items,
            extra,
            source,
            taken_by,
            live_rx,
            first_frame: true,
            held: false,
            wants_focus: false,
            pinned: false,
            flash: None,
            query: String::new(),
            selected: None,
            selected_key: None,
            reveal: false,
            show_extra: false,
            show_hidden: false,
            uses,
            used,
            size_hint: egui::vec2(900.0, 420.0),
            columns: COLUMNS,
            placed: saved_place(),
            moved: false,
            last_rect: None,
            fake_held: 0,
        };
        (p.size_hint, p.columns) = p.wanted_size();
        p
    }

    /// The live menu read finished: use it.
    pub fn poll_live(&mut self) {
        if let Some(rx) = &self.live_rx {
            if let Ok(live) = rx.try_recv() {
                if !live.is_empty() {
                    // Keep the read, so the app is listed and its menus are known after it quits.
                    let (bundle, copy) = (self.app.bundle.clone(), live.clone());
                    std::thread::spawn(move || crate::sources::remember_menus(&bundle, copy));
                    self.items.retain(|s| !matches!(s.origin, Origin::LiveMenu | Origin::RememberedMenu | Origin::MenuFile | Origin::ElectronBundle) || s.scope == Scope::Global);
                    self.used.extend(live.iter().map(|s| s.trigger.clone()));
                    self.items.extend(live);
                    self.source = Origin::LiveMenu.label().to_string();
                    (self.size_hint, self.columns) = self.wanted_size();
                    self.selected = None;
                    self.selected_key = None;
                }
                self.live_rx = None;
            }
        }
    }

    fn font(&self) -> f32 {
        crate::settings::get().panel_font
    }

    /// A window size that fits the rows, up to the maximum share of a 1512 × 982 point screen.
    /// The window code limits it to the real screen.
    fn wanted_size(&self) -> (egui::Vec2, usize) {
        let list = entries(&self.items, &[], "", false);
        let groups = list.iter().map(|e| &e.group).collect::<HashSet<_>>().len();
        let n = columns_for(list.len(), groups, COLUMNS);
        let cols = layout(&list, n);
        let font = self.font();
        let col_w = 300.0 + (font - 13.0) * 18.0;
        let w = (n as f32 * col_w + (n as f32 - 1.0) * 18.0 + 44.0).max(760.0);
        let h = 16.0 * 2.0 + 74.0 + 18.0 + columns_height(&cols, font) + 30.0 + 12.0;
        (egui::vec2(w, h.max(260.0)), n)
    }

    /// Keep where the person moved Peek, for the next time it opens.
    pub fn save_place(&self) {
        if let (true, Some(r)) = (self.moved, self.last_rect) {
            crate::settings::update(|s| {
                s.panel_pos = Some([r.min.x.round(), r.min.y.round()]);
                s.panel_size = Some([r.width().round(), r.height().round()]);
            });
        }
    }

    /// Start with a filter typed (screenshots).
    pub fn set_query(&mut self, q: &str) {
        self.query = q.to_string();
    }

    /// True when Peek lists a row with these keys (for the flash).
    pub fn has_row(&self, t: &Trigger) -> bool {
        self.items.iter().any(|s| s.trigger == *t)
    }

    fn title(&self) -> String {
        self.items
            .first()
            .map(|s| s.app.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| if self.app.name.is_empty() { self.app.bundle.clone() } else { self.app.name.clone() })
    }

    /// Keys typed while Peek has focus. Runs before the search field sees them.
    fn keys(&mut self, ui: &mut egui::Ui, items: &[Shortcut], list: &[Entry], cols: &[Vec<Line>]) -> Option<Outcome> {
        use egui::{Key, Modifiers};
        let order: Vec<usize> = cols.iter().flatten().filter_map(|l| if let Line::Item(i) = l { Some(*i) } else { None }).collect();
        let pos = |i: usize| -> Option<(usize, usize)> {
            cols.iter().enumerate().find_map(|(c, col)| col.iter().filter(|l| matches!(l, Line::Item(_))).position(|l| *l == Line::Item(i)).map(|r| (c, r)))
        };
        let items_in = |c: usize| -> Vec<usize> { cols.get(c).map(|col| col.iter().filter_map(|l| if let Line::Item(i) = l { Some(*i) } else { None }).collect()).unwrap_or_default() };
        let mut out = None;
        let mut sel = self.selected.filter(|s| order.contains(s));
        let mut moved = false;
        ui.input_mut(|i| {
            // Text size: ⌘ + and ⌘ − (before egui uses them to zoom every window).
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
            if i.consume_key(Modifiers::NONE, Key::Tab) {
                set_mode(mode().next());
            }
            if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                sel = match sel.and_then(|s| order.iter().position(|&x| x == s)) {
                    Some(p) => order.get(p + 1).copied().or(sel),
                    None => order.first().copied(),
                };
                moved = true;
            }
            if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                sel = match sel.and_then(|s| order.iter().position(|&x| x == s)) {
                    Some(p) => order.get(p.saturating_sub(1)).copied(),
                    None => order.last().copied(),
                };
                moved = true;
            }
            if let Some((c, r)) = sel.and_then(pos) {
                for (key, step) in [(Key::ArrowLeft, -1i32), (Key::ArrowRight, 1)] {
                    if i.consume_key(Modifiers::NONE, key) {
                        let to = c as i32 + step;
                        if to >= 0 {
                            let col = items_in(to as usize);
                            if let Some(&x) = col.get(r.min(col.len().saturating_sub(1))) {
                                sel = Some(x);
                                moved = true;
                            }
                        }
                    }
                }
            }
            if i.consume_key(Modifiers::ALT, Key::Enter) {
                if let Some(s) = sel {
                    crate::favorites::toggle(list[s].s);
                }
            }
            if i.consume_key(Modifiers::NONE, Key::Enter) {
                // Return with no row selected runs the first row of a filtered list.
                let pick = sel.or_else(|| (!self.query.is_empty()).then(|| order.first().copied()).flatten());
                if let Some(s) = pick {
                    out = Some(Outcome::Run(list[s].s.clone()));
                }
            }
            if i.consume_key(Modifiers::NONE, Key::Escape) {
                if self.query.is_empty() {
                    crate::hotkey::log("peek: Esc");
                    out = Some(Outcome::Close);
                } else {
                    self.query.clear();
                    sel = None;
                }
            }
            // Another shortcut pressed inside Peek: run that row in the app, or pass the keys on.
            if out.is_none() {
                let mut pressed = None;
                for e in &i.events {
                    if let egui::Event::Key { key, pressed: true, modifiers, .. } = e {
                        if let Some(t) = shortcut_from_egui(*key, *modifiers) {
                            pressed = Some(t);
                        }
                    }
                }
                if let Some(t) = pressed {
                    crate::hotkey::log(&format!("peek: keys pressed inside Peek: {}", trigger_text(&t)));
                    out = Some(match items.iter().find(|s| s.trigger == t) {
                        Some(s) => Outcome::Run(s.clone()),
                        None => Outcome::Pass(t),
                    });
                }
            }
        });
        if moved {
            self.reveal = true;
        }
        self.selected = sel;
        out
    }

    /// Draw the panel.
    pub fn draw(&mut self, ui: &mut egui::Ui) -> Outcome {
        // The rows borrow the lists while the key handling changes the panel: lend them out for the frame.
        let title = self.title();
        let items = std::mem::take(&mut self.items);
        let extra = std::mem::take(&mut self.extra);
        let out = self.draw_with(ui, &title, &items, &extra);
        self.items = items;
        self.extra = extra;
        out
    }

    fn draw_with(&mut self, ui: &mut egui::Ui, title: &str, items: &[Shortcut], extra: &[Shortcut]) -> Outcome {
        let t = th();
        let mut outcome = Outcome::Keep;
        let held = if self.fake_held != 0 { self.fake_held } else { crate::hotkey::current_mods() };
        let font = self.font();
        let list = entries(items, if self.show_extra { extra } else { &[] }, &self.query, self.show_hidden);
        if let Some(k) = &self.selected_key {
            let same = self.selected.and_then(|i| list.get(i)).is_some_and(|e| crate::favorites::key(e.s) == *k);
            if !same {
                self.selected = list.iter().position(|e| crate::favorites::key(e.s) == *k);
            }
        }
        let groups = list.iter().map(|e| &e.group).collect::<HashSet<_>>().len();
        // While filtering, keep the column count of the full list, so the rows do not jump.
        let n_cols = if self.query.is_empty() { columns_for(list.len(), groups, COLUMNS) } else { self.columns };
        let mut cols = layout(&list, n_cols.max(1));
        // Keep empty columns while filtering, so a short result keeps its column width.
        while cols.len() < n_cols {
            cols.push(Vec::new());
        }
        let focused = ui.input(|i| i.focused);
        if focused || self.wants_focus {
            if let Some(o) = self.keys(ui, items, &list, &cols) {
                outcome = o;
            }
        }
        let hits = if held != 0 { list.iter().filter(|e| !e.other_tool && matches_held(&e.s.trigger, held)).count() } else { 0 };
        let mut search_id = None;
        egui::Frame::new()
            .fill(t.bg)
            .stroke(Stroke::new(1.0, t.line_strong))
            .inner_margin(egui::Margin::symmetric(22, 16))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                // The two header rows move the window: drag them anywhere, also to another screen.
                // A double click puts Peek back in the middle, at the size that fits its rows.
                // Added first, so the buttons and the filter on top of it still get their clicks.
                let head = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), 64.0));
                let grab = ui.interact(head, egui::Id::unique("peek-move"), egui::Sense::click_and_drag());
                if grab.drag_started() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    self.moved = true;
                }
                if grab.double_clicked() {
                    self.placed = None;
                    self.moved = false;
                    crate::settings::update(|s| {
                        s.panel_pos = None;
                        s.panel_size = None;
                    });
                }
                // Row 1: who, how many, search, pin, close.
                ui.horizontal(|ui| {
                    crate::icons::show(ui, Some(&self.app.bundle), title, 28.0);
                    ui.add_space(2.0);
                    ui.label(semi(title).size(21.0).color(t.text));
                    ui.add_space(2.0);
                    let count = if self.query.is_empty() { format!("{} shortcuts", items.len()) } else { format!("{} of {} shortcuts", list.iter().filter(|e| !e.other_tool).count(), items.len()) };
                    ui.label(RichText::new(format!("{count} · {}", self.source)).color(t.muted));
                    if self.live_rx.is_some() {
                        ui.spinner();
                        ui.label(RichText::new("reading live menus…").color(t.muted));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(medium("Close")).on_hover_text("Esc").clicked() {
                            crate::hotkey::log("peek: Close button");
                            outcome = Outcome::Close;
                        }
                        let pin = ui.add(egui::Button::new(medium(if self.pinned { "Kept open" } else { "Keep open" })).selected(self.pinned));
                        pin.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, self.pinned, "Keep Peek open"));
                        if pin.on_hover_text("Peek stays open while you work and shows the shortcuts of the app in front. Drag the top to move it, the corner to resize it. Esc, Close, or ⌘ ⌘ closes it.").clicked() {
                            self.pinned = !self.pinned;
                        }
                        ui.add_space(4.0);
                        let edit = egui::TextEdit::singleline(&mut self.query)
                            .hint_text("Type to filter")
                            .desired_width(230.0)
                            .margin(egui::Margin::symmetric(8, 4));
                        let r = ui.add(edit);
                        if r.changed() {
                            self.selected = None;
                        }
                        search_id = Some(r.id);
                    });
                });
                ui.add_space(4.0);
                // Row 2: what the held keys do, free keys, and the keys Peek understands.
                ui.horizontal(|ui| {
                    ui.set_min_height(22.0);
                    if held != 0 {
                        let syms = mod_symbols(held).join("");
                        let status = ui.label(semi(format!("Holding {syms} · {}", matches_text(hits))).color(t.accent));
                        // VoiceOver reads the count when it changes.
                        ui.ctx().accesskit_node_builder(status.id, |n| n.set_live(egui::accesskit::Live::Polite));
                        let m = ui.add(egui::Button::new(RichText::new(mode().name()).size(12.0).color(t.text2)).small());
                        if m.on_hover_text("How held keys pick rows. Click or press Tab to change.").clicked() {
                            set_mode(mode().next());
                        }
                        let free = free_with(held, &self.used);
                        if !free.is_empty() {
                            ui.add_space(10.0);
                            ui.label(RichText::new(format!("Free with {syms}:")).size(12.0).color(t.muted));
                            let shown: Vec<&str> = free.iter().take(14).map(String::as_str).collect();
                            let more = free.len().saturating_sub(shown.len());
                            let text = if more > 0 { format!("{}  +{more}", shown.join(" ")) } else { shown.join(" ") };
                            ui.label(RichText::new(text).size(12.0).color(t.good).monospace())
                                .on_hover_text(format!("No shortcut of this app and no system-wide hotkey uses these keys with {syms}."));
                        }
                    } else {
                        ui.label(RichText::new(format!("Hold ⌃ ⌥ ⇧ ⌘ to highlight · {}", mode().name())).size(12.0).color(t.muted));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let hint = if self.held {
                            let add = not_held(held);
                            if add.is_empty() { "Let go to close".to_string() } else { format!("Add {add} to narrow · let go to close") }
                        } else {
                            "↑↓←→ move · ↩ run · ⌥↩ star · ⇥ highlight · ⌘± size · esc close".to_string()
                        };
                        ui.add(egui::Label::new(RichText::new(hint).color(t.muted).size(12.0)).truncate());
                    });
                });
                ui.add_space(6.0);
                let (line, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
                ui.painter().rect_filled(line, 0.0, t.line);
                ui.add_space(10.0);
                let footer_h = 26.0;
                let body_h = (ui.available_height() - footer_h).max(60.0);
                ui.allocate_ui(egui::vec2(ui.available_width(), body_h), |ui| {
                    ui.set_min_height(body_h);
                    if items.is_empty() {
                        ui.add_space(40.0);
                        ui.vertical_centered(|ui| {
                            ui.label(medium("No shortcuts read for this app yet.").size(17.0).color(t.text2));
                            ui.label(RichText::new("Allow Accessibility for Keyclash to read the menus of any open app.").color(t.muted));
                        });
                        return;
                    }
                    if list.is_empty() {
                        ui.add_space(40.0);
                        ui.vertical_centered(|ui| {
                            ui.label(medium(format!("No shortcut matches \"{}\".", self.query)).size(17.0).color(t.text2));
                            ui.label(RichText::new("Esc clears the filter.").color(t.muted));
                        });
                        return;
                    }
                    let v = RowsView {
                        entries: &list,
                        cols: &cols,
                        taken: &self.taken_by,
                        held,
                        uses: &self.uses,
                        selected: self.selected,
                        reveal: self.reveal,
                        flash: self.flash.as_ref().map(|f| &f.0),
                        font,
                        strong: high_contrast(),
                        runnable: true,
                    };
                    match draw_rows(ui, &v) {
                        Some(RowEvent::Run(i)) => outcome = Outcome::Run(list[i].s.clone()),
                        Some(RowEvent::Select(i)) => self.selected = Some(i),
                        None => {}
                    }
                });
                self.reveal = false;
                // Footer: other tools' hotkeys and hidden rows, folded away by default.
                ui.horizontal(|ui| {
                    ui.set_min_height(footer_h - 4.0);
                    if !extra.is_empty() {
                        let tools: Vec<String> = {
                            let mut v: Vec<String> = Vec::new();
                            for s in extra {
                                if !v.contains(&s.app) {
                                    v.push(s.app.clone());
                                }
                            }
                            v
                        };
                        let names = if tools.len() > 3 { format!("{} and {} more", tools[..3].join(", "), tools.len() - 3) } else { tools.join(", ") };
                        let label = format!("{} {}: {} system-wide hotkeys from {names}", if self.show_extra { "▾" } else { "▸" }, ALSO_ACTIVE, extra.len());
                        let b = ui.add(egui::Button::new(RichText::new(label).size(12.0).color(t.text2)).frame(false));
                        b.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, self.show_extra, ALSO_ACTIVE));
                        if b.clicked() {
                            self.show_extra = !self.show_extra;
                        }
                    }
                    let hidden = items.iter().filter(|s| crate::favorites::is_hidden(s)).count();
                    if hidden > 0 {
                        ui.add_space(12.0);
                        let label = format!("{} {hidden} hidden", if self.show_hidden { "▾" } else { "▸" });
                        let b = ui.add(egui::Button::new(RichText::new(label).size(12.0).color(t.text2)).frame(false));
                        b.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, self.show_hidden, "Show hidden rows"));
                        if b.on_hover_text("Rows you hid with a right-click").clicked() {
                            self.show_hidden = !self.show_hidden;
                        }
                    }
                });
            });
        self.selected_key = self.selected.and_then(|i| list.get(i)).map(|e| crate::favorites::key(e.s));
        // The corner grip changes the size.
        let corner = ui.max_rect().right_bottom();
        let grip = egui::Rect::from_min_max(corner - egui::vec2(18.0, 18.0), corner);
        let r = ui.interact(grip, egui::Id::unique("peek-size"), egui::Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeNwSe);
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Resize Peek"));
        {
            let p = ui.painter();
            for k in 0..3 {
                let d = 4.0 + k as f32 * 4.0;
                p.line_segment([corner - egui::vec2(d, 3.0), corner - egui::vec2(3.0, d)], Stroke::new(1.2, if r.hovered() || r.dragged() { t.muted } else { t.faint }));
            }
        }
        if r.dragged() {
            if let Some(inner) = ui.input(|i| i.viewport().inner_rect) {
                let size = (inner.size() + r.drag_delta()).max(egui::vec2(480.0, 220.0));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
                self.moved = true;
            }
        }
        // Remember the window's place, to keep it after a move.
        if let (Some(outer), Some(inner)) = ui.input(|i| (i.viewport().outer_rect, i.viewport().inner_rect)) {
            self.last_rect = Some(egui::Rect::from_min_size(outer.min, inner.size()));
        }
        // Typing goes to the filter while Peek has focus.
        if let Some(id) = search_id {
            if (self.first_frame || focused) && self.wants_focus && !ui.memory(|m| m.has_focus(id)) && ui.memory(|m| m.focused().is_none()) {
                ui.memory_mut(|m| m.request_focus(id));
            }
        }
        self.first_frame = false;
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn highlight_modes() {
        let cmd_t = Trigger::combo(CMD, "T");
        let cmd_shift_t = Trigger::combo(CMD | SHIFT, "T");
        set_mode(HighlightMode::Exact);
        assert!(matches_held(&cmd_t, CMD));
        assert!(!matches_held(&cmd_shift_t, CMD));
        assert!(matches_held(&cmd_shift_t, CMD | SHIFT));
        set_mode(HighlightMode::Includes);
        assert!(matches_held(&cmd_shift_t, CMD));
        assert!(!matches_held(&cmd_t, CMD | SHIFT));
        set_mode(HighlightMode::Any);
        assert!(matches_held(&cmd_t, CMD | SHIFT));
        set_mode(HighlightMode::Exact);
    }

    #[test]
    fn macos_mirror_keys_are_not_marked_taken() {
        let t = Trigger::combo(CTRL | FN, "C");
        let all = [
            Shortcut::new("macOS", "Center", t.clone(), Scope::Global, Origin::SystemSettings).bundle(Some(crate::sources::MACOS)),
            Shortcut::new("OmniWM", "Toggle", Trigger::combo(OPT, "T"), Scope::Global, Origin::ConfigFile).bundle(Some("com.barut.OmniWM")),
        ];
        let taken = taken_by(&all, "com.apple.finder");
        assert!(!taken[&t].blocks(&t, "Center"));
        assert!(taken[&t].blocks(&t, "Centre Text"));
        assert!(taken[&Trigger::combo(OPT, "T")].blocks(&Trigger::combo(OPT, "T"), "Aspect Ratio Grid"));
        assert_eq!(taken[&Trigger::combo(OPT, "T")].app, "OmniWM");
    }

    fn menu(action: &str, path: &str, t: Trigger) -> Shortcut {
        Shortcut::new("Test", action, t, Scope::App, Origin::LiveMenu).bundle(Some("test.app")).detail(format!("{path} › {action}"))
    }

    #[test]
    fn words() {
        assert_eq!(matches_text(1), "1 match");
        assert_eq!(matches_text(3), "3 matches");
        // Holding ⇧⌘: only ⌃ and ⌥ can still narrow.
        assert_eq!(not_held(SHIFT | CMD), "⌃ ⌥");
        assert_eq!(not_held(0), "⌃ ⌥ ⇧ ⌘");
    }

    #[test]
    fn submenus_and_menu_paths() {
        let s = menu("Find Next", "Edit › Find", Trigger::combo(CMD, "G"));
        assert_eq!(group_of(&s), ("Edit".to_string(), Some("Find".to_string())));
        assert_eq!(menu_path(&s).unwrap(), vec!["Edit", "Find", "Find Next"]);
        let s = menu("Undo", "Edit", Trigger::combo(CMD, "Z"));
        assert_eq!(group_of(&s), ("Edit".to_string(), None));
        // A config-file shortcut has no menu to press.
        let c = Shortcut::new("Code", "Go", Trigger::combo(CMD, "P"), Scope::App, Origin::ConfigFile).detail("/x/keybindings.json");
        assert_eq!(menu_path(&c), None);
    }

    #[test]
    fn filter_and_layout() {
        let items = vec![
            menu("Undo", "Edit", Trigger::combo(CMD, "Z")),
            menu("Redo", "Edit", Trigger::combo(CMD | SHIFT, "Z")),
            menu("Find…", "Edit › Find", Trigger::combo(CMD, "F")),
            menu("Find Next", "Edit › Find", Trigger::combo(CMD, "G")),
            menu("Reload", "View", Trigger::combo(CMD, "R")),
        ];
        // Each word must match the action, the menu or the keys.
        let found = entries(&items, &[], "find next", false);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].s.action, "Find Next");
        assert_eq!(entries(&items, &[], "view", false).len(), 1);
        assert_eq!(entries(&items, &[], "shift", false).len(), 1);
        // One column: the Edit group, the Find submenu heading, then View.
        let all = entries(&items, &[], "", false);
        let cols = layout(&all, 1);
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0][0], Line::Head("Edit".into()));
        assert_eq!(cols[0][3], Line::Sub("Find".into()));
        assert!(cols[0].contains(&Line::Head("View".into())));
        // Few rows need few columns; many rows use all four.
        assert_eq!(columns_for(5, 2, 4), 1);
        assert_eq!(columns_for(25, 4, 4), 4);
        assert_eq!(columns_for(400, 9, 4), 4);
    }

    #[test]
    fn one_group_does_not_repeat_its_heading() {
        let items: Vec<Shortcut> = (0..30).map(|i| menu(&format!("Item {i}"), "Edit", Trigger::combo(CMD, format!("{i}")))).collect();
        let all = entries(&items, &[], "", false);
        let cols = layout(&all, 2);
        assert_eq!(cols[1][0], Line::Cont(String::new()));
        // With two groups, the next column names the group it goes on with.
        let mut more = items.clone();
        more.push(menu("Reload", "View", Trigger::combo(CMD, "R")));
        let all = entries(&more, &[], "", false);
        let cols = layout(&all, 2);
        assert_eq!(cols[1][0], Line::Cont("Edit".into()));
    }

    #[test]
    fn by_app_counts_and_shows_mode_keys() {
        let all = vec![
            Shortcut::new("AeroSpace", "focus left", Trigger::combo(OPT, "H"), Scope::Global, Origin::ConfigFile)
                .bundle(Some("bobko.aerospace"))
                .detail("/x/aerospace.toml · [mode.main.binding] alt-h"),
            Shortcut::new("AeroSpace", "reload-config", Trigger::combo(0, "R"), Scope::Mode, Origin::ConfigFile)
                .bundle(Some("bobko.aerospace"))
                .detail("/x/aerospace.toml · [mode.service.binding] r"),
            Shortcut::new("skhd", "open terminal", Trigger::combo(CMD, "Return"), Scope::Global, Origin::ConfigFile),
        ];
        // Peek leaves mode keys out; By app lists them in a group named after the mode.
        assert_eq!(app_items(&all, "bobko.aerospace").len(), 1);
        let items = owner_items(&all, "bobko.aerospace");
        assert_eq!(items.len(), 2);
        assert_eq!(group_of(&items[1]).0, "Service mode");
        // A tool without an app is found by its name.
        assert_eq!(owner_items(&all, "skhd").len(), 1);
    }

    #[test]
    fn few_rows_get_fewer_wider_columns() {
        let items: Vec<Shortcut> = (0..45).map(|i| menu(&format!("Item {i}"), "Edit", Trigger::combo(CMD, format!("{i}")))).collect();
        let all = entries(&items, &[], "", false);
        // 926 points wide fits two columns of 360; 800 points high fits 45 rows in two.
        assert_eq!(fit_columns(&all, 926.0, 800.0, 13.0), 2);
        // A tall pane needs only one.
        assert_eq!(fit_columns(&all, 926.0, 2000.0, 13.0), 1);
        // A narrow pane never gets more columns than fit.
        assert_eq!(fit_columns(&all, 400.0, 200.0, 13.0), 1);
    }

    #[test]
    fn other_tools_come_last_and_closed_tools_are_left_out() {
        let items = vec![menu("Undo", "Edit", Trigger::combo(CMD, "Z"))];
        let all = vec![
            items[0].clone(),
            Shortcut::new("Raycast", "Toggle", Trigger::combo(OPT, "Space"), Scope::Global, Origin::AppSettings).bundle(Some("com.raycast.macos")),
            Shortcut::new("AeroSpace", "Focus left", Trigger::combo(CTRL | OPT, "H"), Scope::Global, Origin::ConfigFile).bundle(Some("bobko.aerospace")),
        ];
        let closed: HashSet<String> = ["bobko.aerospace".to_string()].into_iter().collect();
        let extra = also_active(&all, "test.app", &closed);
        assert_eq!(extra.len(), 1);
        assert_eq!(extra[0].app, "Raycast");
        let list = entries(&items, &extra, "", false);
        assert_eq!(list.last().unwrap().group, ALSO_ACTIVE);
        assert!(list.last().unwrap().other_tool);
    }

    #[test]
    fn a_modifier_key_by_itself_is_not_a_shortcut() {
        let cmd = egui::Modifiers::COMMAND;
        let cmd_shift = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        // Holding ⌘, then adding ⇧ to highlight: Peek stays open (Bobby's report, 2026-11-12).
        for k in [egui::Key::ShiftLeft, egui::Key::ShiftRight, egui::Key::AltLeft, egui::Key::ControlLeft, egui::Key::SuperLeft] {
            assert_eq!(shortcut_from_egui(k, cmd), None, "{k:?}");
            assert_eq!(shortcut_from_egui(k, cmd_shift), None, "{k:?}");
        }
        assert_eq!(shortcut_from_egui(egui::Key::Z, cmd_shift), Some(Trigger::combo(CMD | SHIFT, "Z")));
        // Typing without ⌘ or ⌃ goes to the filter.
        assert_eq!(shortcut_from_egui(egui::Key::Z, egui::Modifiers::SHIFT), None);
    }

    #[test]
    fn free_keys_with_the_held_modifiers() {
        let used: HashSet<Trigger> = [Trigger::combo(CMD | SHIFT, "Z"), Trigger::combo(CMD | SHIFT, "A")].into_iter().collect();
        let free = free_with(CMD | SHIFT, &used);
        assert!(!free.contains(&"Z".to_string()) && !free.contains(&"A".to_string()));
        assert!(free.contains(&"B".to_string()));
        // Shift alone types capitals: nothing is free to suggest.
        assert!(free_with(SHIFT, &used).is_empty());
    }
}
