//! The "Use" tab: which shortcuts you press, which starred ones you do not press,
//! and which key presses match nothing in the scan. The counts come from `usage.rs`.

use crate::model::*;
use crate::theme::{card, dot, medium, mix, num, overline, segmented, semi, th, Theme};
use crate::ui::keycaps;
use crate::usage::{self, Range, Report};
use eframe::egui::{self, Color32, RichText, Stroke};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

/// Rows shown in a list before "Show all".
const SHORT_LIST: usize = 15;

/// How strong a count is next to the largest count, from 0 to 1. A log scale: a key pressed
/// 5 times stays visible next to a key pressed 500 times.
pub fn heat_level(n: u32, max: u32) -> f32 {
    if n == 0 || max == 0 {
        return 0.0;
    }
    (((1.0 + n as f32).ln() / (1.0 + max as f32).ln()).clamp(0.0, 1.0) * 0.88 + 0.12).min(1.0)
}

/// Fill and border for a heat level: orange for some use, red for the most use.
pub fn heat_colors(t: &Theme, level: f32) -> (Color32, Color32) {
    if level <= 0.0 {
        return (t.keycap, t.keycap_line);
    }
    let hue = mix(t.blocks, t.clash, level);
    let strongest = if t.dark { 0.74 } else { 0.5 };
    (t.tint(hue, 0.1 + (strongest - 0.1) * level), t.tint(hue, 0.4 + 0.5 * level))
}

pub struct UseView {
    pub range: Range,
    all_used: bool,
    /// Leave Copy, Paste, Cut, Undo, Redo and Select All out of "Most used".
    hide_universal: bool,
    all_unknown: bool,
    all_ambiguous: bool,
    all_starred: bool,
    /// "Clear counts" was clicked one time: the second click within 4 s clears.
    clear_armed: Option<Instant>,
    /// The report for (count generation, range, scan generation).
    cache: Option<(u64, Range, u64, Arc<Report>, String)>,
}

impl Default for UseView {
    fn default() -> Self {
        UseView { range: Range::All, all_used: false, hide_universal: false, all_unknown: false, all_ambiguous: false, all_starred: false, clear_armed: None, cache: None }
    }
}

impl UseView {
    /// The presses matched to the shortcut list, and the first day counted. Worked out again only
    /// when a key was counted, the range changed, or a scan ended (`scan_gen`).
    pub fn report(&mut self, list: &[Shortcut], scan_gen: u64) -> (Arc<Report>, String) {
        let count_gen = usage::generation();
        if let Some((g, r, s, rep, since)) = &self.cache {
            if *g == count_gen && *r == self.range && *s == scan_gen {
                return (rep.clone(), since.clone());
            }
        }
        let (since, rows) = usage::snapshot(self.range);
        let rep = Arc::new(usage::attribute(list, &rows));
        self.cache = Some((count_gen, self.range, scan_gen, rep.clone(), since.clone()));
        (rep, since)
    }

    /// The range switch, shared with the keyboard map.
    pub fn range_switch(&mut self, ui: &mut egui::Ui) {
        segmented(ui, &mut self.range, &[(Range::Today, "Today", ""), (Range::Week, "7 days", ""), (Range::Month, "30 days", ""), (Range::All, "All", "")]);
    }

    pub fn show(&mut self, ui: &mut egui::Ui, list: &[Shortcut], apps: &[AppInfo], scan_gen: u64) {
        let t = th();
        let (rep, since) = self.report(list, scan_gen);

        // What the counter does now.
        ui.horizontal_wrapped(|ui| {
            let tap = crate::hotkey::TAP_ACTIVE.load(std::sync::atomic::Ordering::Relaxed);
            let (color, text) = if !usage::counting() {
                (t.warn, "This copy does not count. Open Keyclash from your Applications folder.")
            } else if usage::paused() {
                (t.warn, "The use counter is off.")
            } else if !tap {
                (t.warn, "The key listener is off. Allow Accessibility for Keyclash.")
            } else {
                (t.good, "Counting your shortcut presses.")
            };
            dot(ui, color);
            ui.label(medium(text).color(t.text));
            ui.label(RichText::new(format!("{} presses · counting since {}", num(rep.total as usize), usage::day_words(&since))).color(t.muted));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let armed = self.clear_armed.is_some_and(|at| at.elapsed().as_secs_f32() < 4.0);
                let label = if armed { "Click again to clear all counts" } else { "Clear counts" };
                if ui.add(egui::Button::new(RichText::new(label).size(12.5).color(if armed { t.clash } else { t.text2 })).small()).clicked() {
                    if armed {
                        usage::clear();
                        self.clear_armed = None;
                    } else {
                        self.clear_armed = Some(Instant::now());
                    }
                }
                if usage::counting() {
                    let paused = usage::paused();
                    if ui.add(egui::Button::new(RichText::new(if paused { "Turn on the counter" } else { "Turn off the counter" }).size(12.5)).small()).clicked() {
                        usage::set_paused(!paused);
                    }
                }
                ui.add_space(6.0);
                self.range_switch(ui);
            });
        });
        ui.label(RichText::new("Only a key press with ⌘, ⌃, ⌥ or 🌐 is counted. Typing is not recorded. The counts stay in a file on this Mac.").size(12.5).color(t.muted));
        ui.label(RichText::new("Matches use the recorded foreground app and known shortcuts. They do not prove an action ran.").size(12.5).color(t.muted));
        ui.add_space(8.0);

        if rep.total == 0 {
            ui.add_space(48.0);
            ui.vertical_centered(|ui| {
                ui.label(medium("No key presses counted in this range").size(16.0).color(t.text2));
                let hint = if usage::counting() && usage::paused() {
                    "The use counter is off. Turn it on with the button above, or in Settings."
                } else {
                    "Use your shortcuts as usual. This page fills while Keyclash is open."
                };
                ui.label(RichText::new(hint).color(t.muted));
            });
            return;
        }

        egui::ScrollArea::vertical().id_salt("use-page").auto_shrink([false, false]).show(ui, |ui| {
            self.most_used(ui, list, &rep);
            ui.add_space(10.0);
            self.ambiguous(ui, apps, &rep);
            ui.add_space(10.0);
            ui.columns(2, |cols| {
                self.starred_not_used(&mut cols[0], list, &rep);
                self.not_in_scan(&mut cols[1], apps, &rep);
            });
        });
    }

    fn most_used(&mut self, ui: &mut egui::Ui, list: &[Shortcut], rep: &Report) {
        let t = th();
        // One row per shortcut: the same menu item can be in the list from two sources.
        let mut seen: HashSet<String> = HashSet::new();
        let mut rows: Vec<(usize, u32)> = rep.per_shortcut.iter().copied().enumerate().filter(|(_, n)| *n > 0).collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| list[a.0].app.cmp(&list[b.0].app)));
        rows.retain(|(i, _)| seen.insert(crate::favorites::key(&list[*i])));
        let all_groups = group_same_keys(list, &rows);
        let universal_presses: u32 = all_groups.iter().filter(|g| is_universal(&list[g.members[0].0].trigger)).map(|g| g.total).sum();
        let groups: Vec<&KeyGroup> = all_groups.iter().filter(|g| !(self.hide_universal && is_universal(&list[g.members[0].0].trigger))).collect();
        let max = groups.first().map(|g| g.total).unwrap_or(1);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(semi("Matched shortcuts").size(15.0).color(t.text));
                ui.label(RichText::new(format!("{} shortcuts pressed · ☆ stars the ones to keep", groups.len())).size(12.5).color(t.muted));
                if universal_presses > 0 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.checkbox(&mut self.hide_universal, RichText::new(format!("Hide Copy, Paste, Cut, Undo, Redo, Select All ({})", num(universal_presses as usize))).size(12.5).color(t.text2))
                            .on_hover_text("These keys are the same in each app, so they do not show what to change.");
                    });
                }
            });
            ui.add_space(6.0);
            let shown = if self.all_used { groups.len() } else { groups.len().min(SHORT_LIST) };
            egui::Grid::new("most-used").num_columns(5).spacing([14.0, 5.0]).show(ui, |ui| {
                for g in groups.iter().take(shown) {
                    let s = &list[g.members[0].0];
                    if g.members.len() == 1 {
                        crate::panel::star_button(ui, s);
                    } else {
                        // A star belongs to one app's shortcut, so a row for several apps has none.
                        ui.label("");
                    }
                    count_bar(ui, g.total, max);
                    keycaps(ui, &s.trigger, 11.0);
                    let wide = s.scope == Scope::Global;
                    let name = short(&s.action, 60);
                    let text = if wide { medium(name).color(t.blocks) } else { RichText::new(name).color(t.text) };
                    ui.label(text.size(13.0)).on_hover_text(format!("{} · {}", if wide { "System-wide" } else { "Only in this app" }, s.detail));
                    if g.members.len() == 1 {
                        app_name(ui, s.bundle.as_deref(), &s.app);
                    } else {
                        let apps: Vec<String> = g.members.iter().map(|(i, n)| format!("{} — {}", list[*i].app, num(*n as usize))).collect();
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            // The icons of the apps with the most presses.
                            for (i, _) in g.members.iter().take(5) {
                                crate::icons::show(ui, list[*i].bundle.as_deref(), &list[*i].app, 18.0);
                            }
                            ui.add_space(3.0);
                            ui.label(RichText::new(format!("{} apps", g.members.len())).size(13.0).color(t.text2));
                        })
                        .response
                        .on_hover_text(apps.join("\n"));
                    }
                    ui.end_row();
                }
            });
            more_button(ui, &mut self.all_used, groups.len(), shown);
        });
    }

    fn starred_not_used(&mut self, ui: &mut egui::Ui, list: &[Shortcut], rep: &Report) {
        let t = th();
        let mut seen: HashSet<String> = HashSet::new();
        let mut used: HashSet<String> = HashSet::new();
        for (i, s) in list.iter().enumerate() {
            if rep.per_shortcut[i] > 0 {
                used.insert(crate::favorites::key(s));
            }
        }
        let mut rows: Vec<&Shortcut> = list
            .iter()
            .filter(|s| crate::favorites::is_starred(s))
            .filter(|s| {
                let k = crate::favorites::key(s);
                !used.contains(&k) && seen.insert(k)
            })
            .collect();
        rows.sort_by(|a, b| a.app.to_lowercase().cmp(&b.app.to_lowercase()).then_with(|| a.trigger.cmp(&b.trigger)));
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(semi("Starred, no unique match").size(15.0).color(t.text));
            ui.label(RichText::new("Starred shortcuts without a unique key match in this range. Ambiguous presses do not count here.").size(12.5).color(t.muted));
            ui.add_space(6.0);
            if rows.is_empty() {
                let none = crate::favorites::count() == 0;
                ui.label(RichText::new(if none { "No starred shortcuts yet. Click ☆ next to a shortcut to star it." } else { "Each starred shortcut has a unique key match." }).color(t.text2));
                return;
            }
            let shown = if self.all_starred { rows.len() } else { rows.len().min(SHORT_LIST) };
            egui::Grid::new("starred-idle").num_columns(4).spacing([12.0, 5.0]).show(ui, |ui| {
                for s in rows.iter().take(shown) {
                    crate::panel::star_button(ui, s);
                    keycaps(ui, &s.trigger, 11.0);
                    ui.label(RichText::new(short(&s.action, 34)).size(13.0).color(t.text)).on_hover_text(&s.action);
                    app_name(ui, s.bundle.as_deref(), &s.app);
                    ui.end_row();
                }
            });
            more_button(ui, &mut self.all_starred, rows.len(), shown);
        });
    }

    fn ambiguous(&mut self, ui: &mut egui::Ui, apps: &[AppInfo], rep: &Report) {
        if rep.ambiguous.is_empty() { return; }
        let t = th();
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(semi("Pressed, several possible actions").size(15.0).color(t.text));
            ui.label(RichText::new("Each press counts once here. A global tool or the foreground app could handle these keys.").size(12.5).color(t.muted));
            let shown = if self.all_ambiguous { rep.ambiguous.len() } else { rep.ambiguous.len().min(SHORT_LIST) };
            let max = rep.ambiguous[0].2;
            for (trigger, bundle, n) in rep.ambiguous.iter().take(shown) {
                ui.horizontal(|ui| {
                    count_bar(ui, *n, max);
                    keycaps(ui, trigger, 11.0);
                    ui.label(format!("with {} in front", crate::sources::app_name_for(apps, bundle)));
                });
            }
            more_button(ui, &mut self.all_ambiguous, rep.ambiguous.len(), shown);
        });
    }

    fn not_in_scan(&mut self, ui: &mut egui::Ui, apps: &[AppInfo], rep: &Report) {
        let t = th();
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(semi("Pressed, not in the scan").size(15.0).color(t.text));
            ui.label(RichText::new("Keys you pressed that match no shortcut Keyclash read. The app in front possibly has a shortcut that is not read yet.").size(12.5).color(t.muted));
            ui.add_space(6.0);
            if rep.unknown.is_empty() {
                ui.label(RichText::new("Each press matched a shortcut in the list.").color(t.text2));
            }
            let max = rep.unknown.first().map(|r| r.2).unwrap_or(1);
            let shown = if self.all_unknown { rep.unknown.len() } else { rep.unknown.len().min(SHORT_LIST) };
            egui::Grid::new("not-in-scan").num_columns(3).spacing([12.0, 5.0]).show(ui, |ui| {
                for (trigger, bundle, n) in rep.unknown.iter().take(shown) {
                    count_bar(ui, *n, max);
                    keycaps(ui, trigger, 11.0);
                    let app = if bundle == usage::UNKNOWN_APP { "App not known".to_string() } else { crate::sources::app_name_for(apps, bundle) };
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.label(RichText::new("in").size(13.0).color(t.muted));
                        crate::icons::show(ui, Some(bundle), &app, 18.0);
                        ui.label(RichText::new(&app).size(13.0).color(t.text2)).on_hover_text(bundle);
                    });
                    ui.end_row();
                }
            });
            more_button(ui, &mut self.all_unknown, rep.unknown.len(), shown);
            if !rep.standard.is_empty() {
                ui.add_space(8.0);
                ui.label(overline("Standard macOS keys"));
                ui.horizontal_wrapped(|ui| {
                    for (name, n) in &rep.standard {
                        ui.label(RichText::new(format!("{name}: {}", num(*n as usize))).size(12.5).color(t.text2));
                        ui.add_space(8.0);
                    }
                });
            }
        });
    }
}

/// Shortcuts with the same keys and the same action name in several apps ("⌘V Paste"), as one row.
pub struct KeyGroup {
    /// (index in the shortcut list, presses), most pressed first.
    pub members: Vec<(usize, u32)>,
    pub total: u32,
}

/// Put rows with the same keys and action name together. A system-wide shortcut stays by itself.
pub fn group_same_keys(list: &[Shortcut], rows: &[(usize, u32)]) -> Vec<KeyGroup> {
    let mut groups: Vec<KeyGroup> = Vec::new();
    let mut at: std::collections::HashMap<(Trigger, String), usize> = std::collections::HashMap::new();
    for &(i, n) in rows {
        let s = &list[i];
        let key = (s.trigger.clone(), s.action.trim().to_lowercase());
        match at.get(&key) {
            Some(&g) if s.scope == Scope::App && list[groups[g].members[0].0].scope == Scope::App => {
                groups[g].members.push((i, n));
                groups[g].total += n;
            }
            _ => {
                if s.scope == Scope::App {
                    at.insert(key, groups.len());
                }
                groups.push(KeyGroup { members: vec![(i, n)], total: n });
            }
        }
    }
    for g in &mut groups {
        g.members.sort_by(|a, b| b.1.cmp(&a.1));
    }
    groups.sort_by(|a, b| b.total.cmp(&a.total));
    groups
}

/// Keys that do the same thing in each Mac app: Copy, Paste, Cut, Undo, Redo, Select All.
pub fn is_universal(t: &Trigger) -> bool {
    match t {
        Trigger::Combo { mods, key } if *mods == CMD => matches!(key.as_str(), "C" | "V" | "X" | "Z" | "A"),
        Trigger::Combo { mods, key } => *mods == CMD | SHIFT && key == "Z",
        _ => false,
    }
}

/// The count, and a bar that is as long as the count next to the largest count.
fn count_bar(ui: &mut egui::Ui, n: u32, max: u32) {
    let t = th();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (r, _) = ui.allocate_exact_size(egui::vec2(34.0, 16.0), egui::Sense::hover());
        ui.painter().text(r.right_center(), egui::Align2::RIGHT_CENTER, num(n as usize), egui::FontId::proportional(13.0), t.text);
        let (r, _) = ui.allocate_exact_size(egui::vec2(72.0, 16.0), egui::Sense::hover());
        let track = egui::Rect::from_center_size(r.center(), egui::vec2(r.width(), 6.0));
        ui.painter().rect_filled(track, 3.0, t.raised);
        let frac = if max == 0 { 0.0 } else { n as f32 / max as f32 };
        let (fill, _) = heat_colors(t, heat_level(n, max));
        let bar = egui::Rect::from_min_size(track.min, egui::vec2((track.width() * frac).max(3.0), track.height()));
        ui.painter().rect(bar, 3.0, mix(fill, mix(t.blocks, t.clash, heat_level(n, max)), 0.55), Stroke::NONE, egui::StrokeKind::Inside);
    });
}

/// The icon of an app, then its name.
fn app_name(ui: &mut egui::Ui, id: Option<&str>, name: &str) {
    let t = th();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        crate::icons::show(ui, id, name, 18.0);
        ui.label(RichText::new(name).size(13.0).color(t.text2));
    });
}

/// A long action name cut to `max` characters. A grid column is as wide as its longest text.
fn short(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max - 1).collect::<String>() + "…"
}

fn more_button(ui: &mut egui::Ui, all: &mut bool, total: usize, shown: usize) {
    if total <= SHORT_LIST {
        return;
    }
    ui.add_space(4.0);
    let label = if *all { "Show fewer".to_string() } else { format!("Show all {total} ({} more)", total - shown) };
    if ui.add(egui::Button::new(RichText::new(label).size(12.5)).small()).clicked() {
        *all = !*all;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_key_in_several_apps_is_one_row() {
        let sc = |app: &str, action: &str, t: Trigger, scope: Scope| Shortcut::new(app, action, t, scope, Origin::MenuFile);
        let paste = Trigger::combo(CMD, "V");
        let list = [
            sc("T3 Code", "Paste", paste.clone(), Scope::App),
            sc("Chrome", "Paste", paste.clone(), Scope::App),
            sc("Chrome", "New Tab", Trigger::combo(CMD, "T"), Scope::App),
            sc("Slack", "paste", paste.clone(), Scope::App),
            sc("Maccy", "Paste", paste.clone(), Scope::Global),
        ];
        let rows = [(0, 25), (2, 6), (1, 2), (3, 1), (4, 1)];
        let g = group_same_keys(&list, &rows);
        // Paste in 3 apps is one row with 28 presses. The system-wide Paste stays by itself.
        assert_eq!(g.len(), 3);
        assert_eq!((g[0].total, g[0].members.len()), (28, 3));
        assert_eq!(g[0].members[0], (0, 25));
        assert_eq!((g[1].total, g[1].members.len()), (6, 1));
        assert!(is_universal(&paste) && is_universal(&Trigger::combo(CMD | SHIFT, "Z")));
        assert!(!is_universal(&Trigger::combo(CMD, "T")) && !is_universal(&Trigger::combo(CMD | SHIFT, "V")));
    }

    #[test]
    fn heat_grows_with_the_count() {
        assert_eq!(heat_level(0, 50), 0.0);
        assert!(heat_level(1, 500) > 0.1);
        assert!(heat_level(5, 500) < heat_level(50, 500));
        assert_eq!(heat_level(500, 500), 1.0);
        // No use keeps the plain key color.
        let t = &crate::theme::DARK;
        assert_eq!(heat_colors(t, 0.0), (t.keycap, t.keycap_line));
        assert_ne!(heat_colors(t, 0.5).0, heat_colors(t, 1.0).0);
    }
}
