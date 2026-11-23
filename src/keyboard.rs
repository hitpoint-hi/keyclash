//! Keyboard map: hold (or click) modifier keys and every key shows which apps use that
//! combination — free, used by one app, shared, blocked by a system-wide tool, or a clash.
//! The keyboard stays beside a separately scrollable shortcut list, grouped by app.

use crate::conflicts::{Conflict, Kind};
use crate::keys::{key_symbol, mod_symbols};
use crate::model::*;
use crate::theme::{card, chip, medium, overline, semi, th, Theme};
use crate::ui::keycaps;
use eframe::egui::{self, Color32, RichText, Stroke};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Default)]
pub struct KeyboardMap {
    /// Modifiers selected by clicking or by a completed shortcut.
    pub locked: u8,
    pub selected: Option<String>,
    pub starred_only: bool,
    /// Color the keys by how many times you press them, not by clash status.
    pub heat: bool,
    /// Last physical modifier state, used to distinguish a new press from release.
    held_modifiers: u8,
}

/// What a key is used for with the current modifier keys; it picks the key's colors.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Use {
    Free,
    /// Used inside this many apps, with no conflict.
    Apps(usize),
    Shared,
    Blocks,
    Clash,
}

/// An inset focus ring with space on both sides, even when hover thickens the status outline.
fn key_focus_ring(painter: &egui::Painter, face: egui::Rect, t: &Theme) {
    painter.rect_stroke(face.shrink(2.75), 4.25, Stroke::new(1.25, t.text), egui::StrokeKind::Inside);
}

/// Fill and border of a key.
/// The border keeps 3:1 against the card and the key's fill.
fn key_colors(t: &Theme, u: Use) -> (Color32, Color32) {
    match u {
        Use::Free => (t.keycap, t.key_line),
        Use::Apps(n) => (t.tint(t.accent, 0.14 + 0.05 * n.min(5) as f32), t.accent),
        Use::Shared if t.dark => (t.tint(t.shared, 0.42), t.text2),
        Use::Shared => (t.tint(t.shared, 0.34), t.shared),
        Use::Blocks => (t.tint(t.blocks, 0.32), t.blocks),
        Use::Clash => (t.tint(t.clash, 0.38), t.clash),
    }
}

/// Every row is this many key units wide.
const ROW_UNITS: f32 = 14.5;
/// Twelve function keys fill the row next to a 1.5-unit esc key.
const F: f32 = 13.0 / 12.0;

/// (label, key name in triggers, width in key units); modifier keys use their bit as name.
fn rows() -> Vec<Vec<(&'static str, &'static str, f32)>> {
    vec![
        vec![("esc", "Escape", 1.5), ("F1", "F1", F), ("F2", "F2", F), ("F3", "F3", F), ("F4", "F4", F), ("F5", "F5", F), ("F6", "F6", F), ("F7", "F7", F), ("F8", "F8", F), ("F9", "F9", F), ("F10", "F10", F), ("F11", "F11", F), ("F12", "F12", F)],
        vec![("`", "`", 1.0), ("1", "1", 1.0), ("2", "2", 1.0), ("3", "3", 1.0), ("4", "4", 1.0), ("5", "5", 1.0), ("6", "6", 1.0), ("7", "7", 1.0), ("8", "8", 1.0), ("9", "9", 1.0), ("0", "0", 1.0), ("-", "-", 1.0), ("=", "=", 1.0), ("⌫", "Delete", 1.5)],
        vec![("⇥", "Tab", 1.5), ("Q", "Q", 1.0), ("W", "W", 1.0), ("E", "E", 1.0), ("R", "R", 1.0), ("T", "T", 1.0), ("Y", "Y", 1.0), ("U", "U", 1.0), ("I", "I", 1.0), ("O", "O", 1.0), ("P", "P", 1.0), ("[", "[", 1.0), ("]", "]", 1.0), ("\\", "\\", 1.0)],
        vec![("caps", "Caps Lock", 1.75), ("A", "A", 1.0), ("S", "S", 1.0), ("D", "D", 1.0), ("F", "F", 1.0), ("G", "G", 1.0), ("H", "H", 1.0), ("J", "J", 1.0), ("K", "K", 1.0), ("L", "L", 1.0), (";", ";", 1.0), ("'", "'", 1.0), ("↩", "Return", 1.75)],
        vec![("⇧", "#SHIFT", 2.25), ("Z", "Z", 1.0), ("X", "X", 1.0), ("C", "C", 1.0), ("V", "V", 1.0), ("B", "B", 1.0), ("N", "N", 1.0), ("M", "M", 1.0), (",", ",", 1.0), (".", ".", 1.0), ("/", "/", 1.0), ("⇧", "#SHIFT", 2.25)],
        vec![("🌐", "#FN", 1.0), ("⌃", "#CTRL", 1.0), ("⌥", "#OPT", 1.0), ("⌘", "#CMD", 1.25), ("space", "Space", 4.0), ("⌘", "#CMD", 1.25), ("⌥", "#OPT", 1.0), ("←", "Left", 1.0), ("↑", "Up", 1.0), ("↓", "Down", 1.0), ("→", "Right", 1.0)],
    ]
}

fn modifier_bit(name: &str) -> Option<u8> {
    Some(match name {
        "#SHIFT" => SHIFT,
        "#FN" => FN,
        "#CTRL" => CTRL,
        "#OPT" => OPT,
        "#CMD" => CMD,
        _ => return None,
    })
}

fn input_modifiers(modifiers: egui::Modifiers) -> u8 {
    let mut mods = 0;
    for (on, bit) in [(modifiers.ctrl, CTRL), (modifiers.alt, OPT), (modifiers.shift, SHIFT), (modifiers.mac_cmd || modifiers.command, CMD)] {
        if on { mods |= bit; }
    }
    mods
}

pub(crate) fn is_modifier_key(key: egui::Key) -> bool {
    matches!(key, egui::Key::ShiftLeft | egui::Key::ShiftRight | egui::Key::ControlLeft | egui::Key::ControlRight | egui::Key::AltLeft | egui::Key::AltRight | egui::Key::SuperLeft | egui::Key::SuperRight | egui::Key::Fn)
}

impl KeyboardMap {
    fn select_key(&mut self, key: String, mods: u8) {
        self.locked = mods;
        self.selected = Some(key);
    }

    fn observe_modifiers(&mut self, mods: u8) {
        if mods & !self.held_modifiers != 0 {
            self.selected = None;
            self.locked = 0;
        }
        self.held_modifiers = mods;
    }

    /// Keep a completed shortcut selected after its keys are released.
    pub fn handle_input(&mut self, input: &mut egui::RawInput, held: u8) {
        prepare_scroll(input);
        input.events.retain(|event| {
            if let egui::Event::ModifiersChanged(modifiers) = event {
                self.observe_modifiers(input_modifiers(*modifiers) | (held & FN));
                return true;
            }
            let egui::Event::Key { key, physical_key, pressed: true, repeat, modifiers, .. } = event else { return true };
            let mut mods = input_modifiers(*modifiers) | (held & FN);
            self.observe_modifiers(mods);
            if is_modifier_key(*key) || physical_key.is_some_and(is_modifier_key) {
                if !repeat {
                    self.selected = None;
                    self.locked = 0;
                }
                return true;
            }
            if mods == 0 {
                // Preserve keyboard navigation through the page's controls.
                if matches!(key, egui::Key::Tab | egui::Key::Enter | egui::Key::Space | egui::Key::ArrowUp | egui::Key::ArrowDown | egui::Key::ArrowLeft | egui::Key::ArrowRight | egui::Key::PageUp | egui::Key::PageDown | egui::Key::Home | egui::Key::End) {
                    return true;
                }
                if *key == egui::Key::Escape {
                    self.selected = None;
                    return false;
                }
                mods = self.locked;
            }
            if !repeat {
                let key = physical_key.unwrap_or(*key);
                let name = if key == egui::Key::Delete { "Forward Delete".to_string() } else { crate::keys::normalize_key(key.name()) };
                let name = if physical_key.is_some() {
                    crate::layout::key_on(&name)
                } else {
                    crate::keys::shifted_base(&name).unwrap_or(name)
                };
                self.select_key(name, mods);
            }
            false
        });
    }

    pub fn show(&mut self, ui: &mut egui::Ui, list: &[Shortcut], conflicts: &[Conflict], usev: &mut crate::use_tab::UseView, used: &crate::usage::Report) {
        let t = th();
        let held = crate::hotkey::current_mods() | input_modifiers(ui.input(|i| i.modifiers));
        let mods = self.locked | held;
        let heat = self.heat;

        // Presses per key: for exactly these modifier keys, or for all combinations when none is selected.
        let mut presses: HashMap<&str, u32> = HashMap::new();
        if heat {
            for (tr, n) in &used.by_trigger {
                if let Trigger::Combo { mods: m, key } = tr {
                    if mods == 0 || *m == mods {
                        *presses.entry(key.as_str()).or_default() += n;
                    }
                }
            }
        }
        let most = presses.values().copied().max().unwrap_or(0);

        // Worst conflict per trigger.
        let mut kind: HashMap<&Trigger, Kind> = HashMap::new();
        for c in conflicts {
            let e = kind.entry(&c.trigger).or_insert(c.kind);
            if c.kind < *e {
                *e = c.kind;
            }
        }
        // Shortcuts on each key for exactly these modifiers.
        let mut on_key: HashMap<String, Vec<&Shortcut>> = HashMap::new();
        for s in list.iter().filter(|s| s.enabled && s.scope != Scope::Mode) {
            if self.starred_only && !crate::favorites::is_starred(s) {
                continue;
            }
            if let Trigger::Combo { mods: m, key } = &s.trigger {
                if *m == mods {
                    on_key.entry(key.clone()).or_default().push(s);
                }
            }
        }

        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Hold modifiers, or press a full shortcut").color(t.text2))
                .on_hover_text("Click modifiers to switch them on. Press a full shortcut to keep its matches visible after you release the keys.");
            ui.add_space(4.0);
            for (bit, name, color) in [(CTRL, "⌃  control", t.ctrl), (OPT, "⌥  option", t.opt), (SHIFT, "⇧  shift", t.shift), (CMD, "⌘  command", t.cmd), (FN, "fn", t.fnk)] {
                if chip(ui, mods & bit != 0, name, color).clicked() {
                    self.locked ^= bit;
                    self.selected = None;
                }
            }
            if (self.locked != 0 || self.selected.is_some()) && ui.add(egui::Button::new(RichText::new("Clear").size(12.5)).small()).clicked() {
                self.locked = 0;
                self.selected = None;
            }
            if let Some(layout) = crate::layout::active_name() {
                ui.add_space(4.0);
                ui.label(RichText::new(format!("Layout: {layout}")).size(12.5).color(t.muted))
                    .on_hover_text("The keys show what they type in the macOS keyboard layout that is selected now. A keyboard that changes the letters by itself is not visible to macOS.");
            }
            ui.add_space(8.0);
            ui.checkbox(&mut self.starred_only, RichText::new(format!("★ Starred only ({})", crate::favorites::count())).size(12.5).color(t.text2));
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Colors").size(12.5).color(t.muted));
            crate::theme::segmented(ui, &mut self.heat, &[(false, "Status", "Colors show free keys and clashes"), (true, "Use", "Colors show how many times you press each key")]);
            if self.heat {
                ui.add_space(10.0);
                ui.label(RichText::new("Presses from").size(12.5).color(t.muted));
                usev.range_switch(ui);
            }
        });
        ui.add_space(4.0);

        let width = ui.available_width();
        let height = ui.available_height();
        let gap = 16.0;
        let keyboard_width = (width * 0.56).clamp(450.0, 900.0).min(width - gap - 320.0);
        let (bounds, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
        let left = egui::Rect::from_min_size(bounds.min, egui::vec2(keyboard_width, height));
        let right = egui::Rect::from_min_max(egui::pos2(left.right() + gap, bounds.top()), bounds.max);
        ui.scope_builder(egui::UiBuilder::new().id_salt("keyboard-pane").max_rect(left), |ui| {
            ui.set_clip_rect(ui.clip_rect().intersect(left));
            egui::ScrollArea::vertical().id_salt("keyboard-drawing").auto_shrink([false, false]).show(ui, |ui| {
                card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        if heat {
                            let label = if mods == 0 { "All combinations".to_string() } else { format!("{} + key", mod_symbols(mods).join("")) };
                            let sum: u32 = presses.values().sum();
                            ui.label(semi(format!("{label}: {} presses on {} keys", crate::theme::num(sum as usize), presses.len())).color(t.text));
                            ui.add_space(10.0);
                            ui.label(RichText::new("fewer").color(t.muted).size(12.0));
                            for level in [0.0, 0.25, 0.5, 0.75, 1.0] {
                                let (fill, line) = crate::use_tab::heat_colors(t, level);
                                let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                                ui.painter().rect(r, 4.0, fill, Stroke::new(1.0, line), egui::StrokeKind::Inside);
                            }
                            ui.label(RichText::new("more presses").color(t.muted).size(12.0));
                            if mods == 0 {
                                ui.add_space(8.0);
                                ui.label(RichText::new("Hold or click modifier keys to see one combination.").color(t.muted).size(12.0));
                            }
                        }
                        let label = if mods == 0 { "Keys used by themselves".to_string() } else { format!("{} + key", mod_symbols(mods).join("")) };
                        if !heat {
                            ui.label(semi(format!("{label}: {} keys in use", on_key.len())).color(t.text));
                            ui.add_space(10.0);
                        }
                    });
                    ui.add_space(8.0);

                    // The keyboard, centred in the card.
                    let gap = 5.0;
                    let unit = (ui.available_width() / ROW_UNITS).min(60.0);
                    let pad = ((ui.available_width() - unit * ROW_UNITS) / 2.0).max(0.0);
                    let row_gaps = 5.0 * ui.spacing().item_spacing.y;
                    let room = height - (ui.cursor().top() - left.top()) - 12.0 - row_gaps;
                    let key_h = (unit * 0.9).round().max(36.0).min((room / 6.0).floor().max(28.0));
                    let compact = unit < 36.0 || key_h < 36.0;
                    for row in rows() {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            if pad > 0.0 {
                                ui.add_space(pad - gap);
                            }
                            for (label, name, w) in row {
                                // The drawing is a US keyboard. A character key shows what it types in the layout in use.
                                let typed = crate::layout::key_on(name);
                                let (label, name) = if typed == name { (label, name) } else { (typed.as_str(), typed.as_str()) };
                                let size = egui::vec2(unit * w - gap, key_h);
                                let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
                                // Room for the selection ring outside the key. Keys sit 5 points apart.
                                let painter = ui.painter_at(rect.expand(4.0));
                                let face = egui::Rect::from_min_max(rect.min, rect.max - egui::vec2(0.0, 1.5));
                                if let Some(bit) = modifier_bit(name) {
                                    let on = mods & bit != 0;
                                    let name = format!("{} modifier", crate::keys::mod_words(bit));
                                    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, ui.is_enabled(), on, &name));
                                    let color = match bit {
                                        CTRL => t.ctrl,
                                        OPT => t.opt,
                                        SHIFT => t.shift,
                                        CMD => t.cmd,
                                        _ => t.fnk,
                                    };
                                    let (fill, line) = if on { (t.tint(color, 0.3), color) } else { (t.keycap, t.key_line) };
                                    painter.rect_filled(face.translate(egui::vec2(0.0, 1.5)), 7.0, t.keycap_edge);
                                    painter.rect(face, 7.0, fill, Stroke::new(if on { 1.5 } else { 1.0 }, line), egui::StrokeKind::Inside);
                                    let fg = if on { t.text } else if resp.hovered() { color } else { t.muted };
                                    painter.text(face.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(15.0), fg);
                                    if resp.has_focus() {
                                        key_focus_ring(&painter, face, t);
                                    }
                                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                        self.locked ^= bit;
                                    self.selected = None;
                                    }
                                    continue;
                                }
                                let users = on_key.get(name).cloned().unwrap_or_default();
                                let owners: HashSet<String> = users.iter().map(|s| s.owner()).collect();
                                let trig = Trigger::Combo { mods, key: name.to_string() };
                                let usage = if users.is_empty() {
                                    Use::Free
                                } else {
                                    match kind.get(&trig) {
                                        Some(Kind::Clash) => Use::Clash,
                                        Some(Kind::Blocks) => Use::Blocks,
                                        Some(Kind::Shared) | Some(Kind::SameModifier) => Use::Shared,
                                        _ => Use::Apps(owners.len()),
                                    }
                                };
                                let pressed = presses.get(name).copied().unwrap_or(0);
                                let (fill, line) = if heat { crate::use_tab::heat_colors(t, crate::use_tab::heat_level(pressed, most)) } else { key_colors(t, usage) };
                                let selected = self.selected.as_deref() == Some(name);
                                let status = match usage {
                                    Use::Free => "free",
                                    Use::Apps(_) => "used inside an app",
                                    Use::Shared => "shared by apps",
                                    Use::Blocks => "blocked by a global shortcut",
                                    Use::Clash => "conflicting global shortcuts",
                                };
                                let accessible = format!("{}: {status}, {} apps, {pressed} presses", crate::keys::trigger_words(&trig), owners.len());
                                resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, &accessible));
                                // Preserve the status outline when drawing hover, selection, and focus.
                                let width = if resp.hovered() { 2.0 } else { 1.0 };
                                painter.rect_filled(face.translate(egui::vec2(0.0, 1.5)), 7.0, t.keycap_edge);
                                painter.rect(face, 7.0, fill, Stroke::new(width, line), egui::StrokeKind::Inside);
                                if selected {
                                    // A 1-point gap keeps the ring apart from the status outline.
                                    painter.rect_stroke(face.expand(1.0), 8.0, Stroke::new(2.0, t.text), egui::StrokeKind::Outside);
                                }
                                let fg = if (heat && pressed == 0) || (!heat && usage == Use::Free) { t.muted } else { t.text };
                                // Symbols such as ⇥ and ↩ come from a fallback font that draws them small.
                                let label_size = if label.is_ascii() { 13.5 } else { 16.0 };
                                if compact {
                                    painter.text(face.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(label_size), fg);
                                } else {
                                    painter.text(face.left_top() + egui::vec2(7.0, 5.0), egui::Align2::LEFT_TOP, label, egui::FontId::proportional(label_size), fg);
                                }
                                // The small number: presses in the Use colors, apps in the Status colors.
                                // It uses the label color: text2 drops under 4.5:1 on the strongest fills.
                                let small = if heat { pressed as usize } else { owners.len() };
                                if small > 0 && !compact {
                                    painter.text(
                                        face.right_bottom() - egui::vec2(7.0, 5.0),
                                        egui::Align2::RIGHT_BOTTOM,
                                        crate::theme::num(small),
                                        egui::FontId::proportional(11.0),
                                        fg,
                                    );
                                }
                                // Drawn last, so the key face does not cover it.
                                if resp.has_focus() {
                                    key_focus_ring(&painter, face, t);
                                }
                                let resp = resp.on_hover_ui(|ui| {
                                    ui.label(semi(format!("{}{}", mod_symbols(mods).join(""), key_symbol(name))));
                                    if heat {
                                        ui.label(format!("{} presses", crate::theme::num(pressed as usize)));
                                        if mods == 0 {
                                            // The combinations with this key, most pressed first.
                                            let mut combos: Vec<(&Trigger, u32)> = used
                                                .by_trigger
                                                .iter()
                                                .filter(|(tr, _)| matches!(tr, Trigger::Combo { key, .. } if key == name))
                                                .map(|(tr, n)| (tr, *n))
                                                .collect();
                                            combos.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
                                            for (tr, n) in combos.into_iter().take(6) {
                                                ui.label(format!("{}  {n}", crate::keys::trigger_text(tr)));
                                            }
                                            return;
                                        }
                                    }
                                    if users.is_empty() {
                                        ui.label("Free: no app we read uses it.");
                                    }
                                    for s in users.iter().take(10) {
                                        ui.label(format!("{} — {}", s.app, s.action));
                                    }
                                    if users.len() > 10 {
                                        ui.label(format!("+{} more", users.len() - 10));
                                    }
                                });
                                if resp.clicked() {
                                    if selected {
                                        self.selected = None;
                                    } else {
                                        let click_mods = ui.input(|i| i.events.iter().rev().find_map(|event| {
                                            if let egui::Event::PointerButton { button: egui::PointerButton::Primary, pressed: false, modifiers, .. } = event {
                                                Some(input_modifiers(*modifiers))
                                            } else {
                                                None
                                            }
                                        })).unwrap_or(held);
                                        self.select_key(name.to_string(), self.locked | click_mods | (mods & FN));
                                    }
                                    ui.ctx().request_repaint();
                                }
                            }
                        });
                    }

                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        for (u, name) in if heat { Vec::new() } else { vec![
                            (Use::Free, "free"),
                            (Use::Apps(2), "inside an app"),
                            (Use::Shared, "several apps, each in its own app"),
                            (Use::Blocks, "a system-wide tool takes it"),
                            (Use::Clash, "clash"),
                        ] } {
                            let (fill, line) = key_colors(t, u);
                            let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                            ui.painter().rect(r, 4.0, fill, Stroke::new(1.0, line), egui::StrokeKind::Inside);
                            ui.label(RichText::new(name).color(t.muted).size(12.0));
                            ui.add_space(4.0);
                        }
                    });

                    // Modifier used by itself (double-tap / hold), for the held modifier.
                    if mods != 0 {
                        let solo: Vec<&Shortcut> = list
                            .iter()
                            .filter(|s| s.enabled && s.trigger.bare_modifier().is_some_and(|m| m & mods != 0))
                            .collect();
                        if !solo.is_empty() {
                            ui.add_space(6.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new("By itself").color(t.muted).size(12.5));
                                for s in solo {
                                    keycaps(ui, &s.trigger, 11.0);
                                    ui.label(RichText::new(format!("{} — {}", s.app, s.action)).color(t.text2).size(12.5));
                                    ui.add_space(10.0);
                                }
                            });
                        }
                    }
                });
            });
        });
        ui.scope_builder(egui::UiBuilder::new().id_salt("shortcut-pane").max_rect(right), |ui| {
            ui.set_clip_rect(ui.clip_rect().intersect(right));
            self.show_matches(ui, mods, &on_key);
        });
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(if held != 0 { 60 } else { 250 }));
    }

    fn show_matches(&mut self, ui: &mut egui::Ui, mods: u8, on_key: &HashMap<String, Vec<&Shortcut>>) {
        let t = th();
        // The matching shortcuts, by app.
        let mut by_app: BTreeMap<String, Vec<&Shortcut>> = BTreeMap::new();
        for (k, v) in on_key {
            if self.selected.as_deref().is_some_and(|sel| sel != k) {
                continue;
            }
            for s in v {
                by_app.entry(s.app.clone()).or_default().push(s);
            }
        }
        let title = match &self.selected {
            Some(k) => format!("Apps using {}{}", mod_symbols(mods).join(""), key_symbol(k)),
            None => format!("All {} shortcuts", if mods == 0 { "plain-key".to_string() } else { mod_symbols(mods).join("") }),
        };
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(semi(title).size(15.0).color(t.text));
                if self.selected.is_some() && ui.small_button("Show all keys").clicked() {
                    self.selected = None;
                }
            });
            let count: usize = by_app.values().map(Vec::len).sum();
            ui.label(RichText::new(format!("{} shortcuts · {} apps", crate::theme::num(count), by_app.len())).size(12.5).color(t.muted));
            ui.add_space(8.0);
            let filter = (mods, self.selected.clone(), self.starred_only);
            egui::ScrollArea::vertical()
                .id_salt(("keyboard-shortcuts", filter))
                .auto_shrink([false, false])
                .min_scrolled_height(0.0)
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    if by_app.is_empty() {
                        ui.label(RichText::new("No matching shortcuts. Hold or click a modifier, select another key, or turn off Starred only.").color(t.muted));
                    }
                    for (app, mut items) in by_app {
                        items.sort_by(|a, b| a.trigger.cmp(&b.trigger).then_with(|| a.action.cmp(&b.action)));
                        ui.horizontal(|ui| {
                            crate::icons::show(ui, None, &app, 16.0);
                            ui.label(overline(&app));
                        });
                        for s in items {
                            ui.horizontal(|ui| {
                                crate::panel::star_button(ui, s);
                                keycaps(ui, &s.trigger, 11.0);
                                let global = s.scope == Scope::Global;
                                let text = if global { medium(&s.action).color(t.blocks) } else { RichText::new(&s.action).color(t.text) };
                                ui.add(egui::Label::new(text.size(13.0)).wrap())
                                    .on_hover_text(format!("{} · {}", if global { "System-wide" } else { "Only in this app" }, s.detail));
                            });
                        }
                        ui.add_space(12.0);
                    }
                });
        });
    }
}

/// Modifiers select the map's contents. They must not turn its wheel events into zoom or horizontal scrolling.
pub fn prepare_scroll(input: &mut egui::RawInput) {
    for event in &mut input.events {
        if let egui::Event::MouseWheel { modifiers, delta, .. } = event {
            // macOS can already put Shift-scroll on the horizontal axis.
            *delta = egui::vec2(0.0, delta.x + delta.y);
            *modifiers = egui::Modifiers::NONE;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_hover_and_focus_keep_the_status_outline_visible() {
        use egui::accesskit::{Action, ActionRequest, NodeId, TreeId};

        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut map = KeyboardMap { selected: Some("F1".into()), ..Default::default() };
        let mut use_view = crate::use_tab::UseView::default();
        let list = [Shortcut::new("Test", "Action", Trigger::combo(0, "F1"), crate::model::Scope::App, crate::model::Origin::MenuFile)];
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1240.0, 820.0))), ..Default::default() };
        let mut draw = |events| {
            let mut output = ctx.run_ui(egui::RawInput { events, ..input.clone() }, |ui| {
                egui::CentralPanel::default().show(ui, |ui| map.show(ui, &list, &[], &mut use_view, &Default::default()));
            });
            output.textures_delta.clear();
            output
        };
        draw(vec![]);
        let resting = draw(vec![]);
        let nodes = &resting.platform_output.accesskit_update.as_ref().unwrap().nodes;
        let (key_id, _) = nodes.iter().find(|(_, node)| {
            node.label().is_some_and(|label| label.contains("f1: used inside an app"))
        }).expect("the shortcut key is exposed to accessibility tools");
        let (modifier_id, modifier) = nodes.iter().find(|(_, node)| {
            node.label() == Some("Command modifier")
        }).expect("the Command key is exposed to accessibility tools");
        let (key_id, modifier_id) = (*key_id, *modifier_id);
        let focus = |target_node: NodeId| egui::Event::AccessKitActionRequest(ActionRequest {
            action: Action::Focus,
            target_tree: TreeId::ROOT,
            target_node,
            data: None,
        });
        let modifier = modifier.bounds().unwrap();
        let modifier_center = egui::pos2((modifier.x0 + modifier.x1) as f32 / 2.0, (modifier.y0 + modifier.y1) as f32 / 2.0);
        let rects = |output: egui::FullOutput| -> Vec<_> {
            output.shapes.into_iter().filter_map(|shape| {
                if let egui::epaint::Shape::Rect(rect) = shape.shape { Some(rect) } else { None }
            }).collect()
        };
        // Another test can change the process-wide theme, so match the palette used in each frame.
        let is_status = |rect: &&egui::epaint::RectShape| {
            [&crate::theme::DARK, &crate::theme::LIGHT].into_iter()
                .any(|t| (rect.fill, rect.stroke.color) == key_colors(t, Use::Apps(1)))
        };
        let resting = rects(resting);
        let key = resting.iter().find(is_status).expect("F1 has a status outline");
        assert_eq!(key.stroke.width, 1.0);
        draw(vec![egui::Event::PointerMoved(key.rect.center())]);
        let active = draw(vec![focus(key_id)]);
        assert_eq!(active.platform_output.accesskit_update.as_ref().unwrap().focus, key_id);
        let active = rects(active);
        let key = active.iter().find(is_status).expect("hover keeps the F1 status color");
        assert_eq!(key.stroke.width, 2.0);
        let has_text_color = |rect: &egui::epaint::RectShape| {
            rect.stroke.color == crate::theme::DARK.text || rect.stroke.color == crate::theme::LIGHT.text
        };
        assert!(active.iter().any(|rect| {
            has_text_color(rect) && rect.stroke_kind == egui::StrokeKind::Outside && rect.rect == key.rect.expand(1.0)
        }), "selection has a ring outside the status outline");
        assert!(active.iter().any(|rect| {
            has_text_color(rect) && rect.stroke_kind == egui::StrokeKind::Inside
                && rect.rect.center() == key.rect.center() && rect.rect.left() > key.rect.left() + key.stroke.width
        }), "focus has a ring inside and separate from the hovered outline");

        let modifier_focused = draw(vec![focus(modifier_id)]);
        assert_eq!(modifier_focused.platform_output.accesskit_update.as_ref().unwrap().focus, modifier_id);
        assert!(rects(modifier_focused).iter().any(|rect| {
            has_text_color(rect) && rect.stroke_kind == egui::StrokeKind::Inside
                && rect.rect.contains(modifier_center)
        }), "the modifier also draws a visible focus ring");
    }

    #[test]
    fn key_outlines_and_labels_have_contrast() {
        use crate::theme::{DARK, LIGHT, contrast};
        for t in [&DARK, &LIGHT] {
            for u in [Use::Free, Use::Apps(1), Use::Apps(3), Use::Apps(5), Use::Shared, Use::Blocks, Use::Clash] {
                let (fill, line) = key_colors(t, u);
                let c = contrast(line, t.card);
                assert!(c >= 3.0, "{u:?} outline on the card (dark={}) has contrast {c:.2}", t.dark);
                let c = contrast(line, fill);
                assert!(c >= 3.0, "{u:?} outline on its fill (dark={}) has contrast {c:.2}", t.dark);
                // The key label and its small count share this color.
                let label = if u == Use::Free { t.muted } else { t.text };
                let c = contrast(label, fill);
                assert!(c >= 4.5, "{u:?} key label (dark={}) has contrast {c:.2}", t.dark);
                // Selection and focus rings use the text color.
                let c = contrast(t.text, t.card).min(contrast(t.text, fill));
                assert!(c >= 3.0, "{u:?} selection ring (dark={}) has contrast {c:.2}", t.dark);
            }
        }
    }

    fn text_rect(output: &egui::FullOutput, text: &str) -> egui::Rect {
        output.shapes.iter().find_map(|shape| {
            if let egui::epaint::Shape::Text(shape) = &shape.shape {
                if shape.galley.text() == text {
                    return Some(shape.galley.rect.translate(shape.pos.to_vec2()));
                }
            }
            None
        }).unwrap_or_else(|| panic!("Missing rendered text: {text}"))
    }

    fn key_event(key: egui::Key, modifiers: egui::Modifiers, pressed: bool) -> egui::Event {
        egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers }
    }

    #[test]
    fn clicking_g_while_holding_modifiers_retains_the_complete_shortcut() {
        for release_with_click in [false, true] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let mut map = KeyboardMap::default();
            let modifiers = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
            let mut list: Vec<_> = (0..29).map(|i| Shortcut::new(&format!("App {i:02}"), "Selected G action", Trigger::combo(CMD | SHIFT, "G"), Scope::App, Origin::MenuFile)).collect();
            list.push(Shortcut::new("Plain G app", "Plain G action", Trigger::combo(0, "G"), Scope::App, Origin::MenuFile));
            fn draw(ctx: &egui::Context, map: &mut KeyboardMap, list: &[Shortcut], events: Vec<egui::Event>, held: u8) -> egui::FullOutput {
                let mut input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1240.0, 620.0))), events, ..Default::default() };
                map.handle_input(&mut input, held);
                let mut output = ctx.run_ui(input, |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        map.show(ui, list, &[], &mut Default::default(), &Default::default());
                    });
                });
                output.textures_delta.clear();
                output
            }
            draw(&ctx, &mut map, &list, vec![egui::Event::ModifiersChanged(modifiers)], CMD | SHIFT);
            let before = draw(&ctx, &mut map, &list, vec![], CMD | SHIFT);
            let pointer = text_rect(&before, "G").center();
            draw(&ctx, &mut map, &list, vec![egui::Event::PointerMoved(pointer)], CMD | SHIFT);
            draw(&ctx, &mut map, &list, vec![egui::Event::PointerButton { pos: pointer, button: egui::PointerButton::Primary, pressed: true, modifiers }], CMD | SHIFT);
            let mut released = vec![egui::Event::PointerButton { pos: pointer, button: egui::PointerButton::Primary, pressed: false, modifiers }];
            if release_with_click {
                released.push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
            }
            draw(&ctx, &mut map, &list, released, if release_with_click { 0 } else { CMD | SHIFT });
            assert_eq!(map.selected.as_deref(), Some("G"));
            assert_eq!(map.locked, CMD | SHIFT, "Clicking G selected the key but did not retain its modifiers");
            if !release_with_click {
                draw(&ctx, &mut map, &list, vec![
                    key_event(egui::Key::ShiftLeft, egui::Modifiers::COMMAND, false),
                    egui::Event::ModifiersChanged(egui::Modifiers::COMMAND),
                ], CMD);
                draw(&ctx, &mut map, &list, vec![
                    key_event(egui::Key::SuperLeft, egui::Modifiers::NONE, false),
                    egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                ], 0);
            }
            let after = draw(&ctx, &mut map, &list, vec![], 0);
            text_rect(&after, "Apps using ⇧⌘G");
            text_rect(&after, "29 shortcuts · 29 apps");
            let update = after.platform_output.accesskit_update.unwrap();
            for modifier in ["Command modifier", "Shift modifier"] {
                let nodes: Vec<_> = update.nodes.iter().map(|(_, n)| n).filter(|n| n.label() == Some(modifier)).collect();
                assert!(!nodes.is_empty(), "Missing {modifier}");
                assert!(nodes.iter().all(|n| n.toggled() == Some(egui::accesskit::Toggled::True)), "The keyboard lost its {modifier} highlight");
            }
        }
    }

    #[test]
    fn modifier_keys_never_become_the_selected_shortcut_key() {
        for key in [egui::Key::ShiftLeft, egui::Key::ShiftRight, egui::Key::ControlLeft, egui::Key::ControlRight, egui::Key::AltLeft, egui::Key::AltRight, egui::Key::SuperLeft, egui::Key::SuperRight, egui::Key::Fn] {
            let mut map = KeyboardMap { locked: CMD | SHIFT, selected: Some("A".into()), ..Default::default() };
            let mut input = egui::RawInput { events: vec![key_event(key, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, true)], ..Default::default() };
            map.handle_input(&mut input, CMD | SHIFT);
            assert_eq!(map.selected, None, "{key:?} became a shortcut key");
            assert_eq!(map.locked, 0, "A new modifier press must replace the previous captured modifiers");
        }
    }

    #[test]
    fn a_new_modifier_chord_shows_all_keys_after_a_completed_shortcut() {
        let modifiers = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        let mut map = KeyboardMap::default();
        let mut input = egui::RawInput { events: vec![
            egui::Event::ModifiersChanged(modifiers),
            key_event(egui::Key::A, modifiers, true),
            key_event(egui::Key::A, modifiers, false),
            egui::Event::ModifiersChanged(egui::Modifiers::COMMAND),
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
        ], ..Default::default() };
        map.handle_input(&mut input, 0);
        assert_eq!(map.selected.as_deref(), Some("A"), "Release must retain a completed shortcut");
        input.events = vec![
            egui::Event::ModifiersChanged(egui::Modifiers::COMMAND),
            key_event(egui::Key::SuperLeft, egui::Modifiers::COMMAND, true),
            key_event(egui::Key::ShiftLeft, modifiers, true),
            egui::Event::ModifiersChanged(modifiers),
        ];
        map.handle_input(&mut input, CMD | SHIFT);
        assert_eq!(map.selected, None, "Holding modifiers must return to all matching keys");
        assert_eq!(map.locked, 0);
        let ctx = egui::Context::default();
        let list = [
            Shortcut::new("Test", "Action on A", Trigger::combo(CMD | SHIFT, "A"), Scope::App, Origin::MenuFile),
            Shortcut::new("Test", "Action on B", Trigger::combo(CMD | SHIFT, "B"), Scope::App, Origin::MenuFile),
        ];
        input.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1240.0, 620.0)));
        for _ in 0..2 {
            let mut output = ctx.run_ui(input.clone(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    map.show(ui, &list, &[], &mut Default::default(), &Default::default());
                });
            });
            output.textures_delta.clear();
            text_rect(&output, "All ⇧⌘ shortcuts");
            text_rect(&output, "2 shortcuts · 1 apps");
            text_rect(&output, "Action on A");
            text_rect(&output, "Action on B");
        }
    }

    #[test]
    fn a_complete_shortcut_keeps_all_seventeen_matches_after_release() {
        let mut map = KeyboardMap::default();
        let modifiers = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        let mut input = egui::RawInput { events: vec![egui::Event::ModifiersChanged(modifiers), key_event(egui::Key::A, modifiers, true)], ..Default::default() };
        map.handle_input(&mut input, CMD | SHIFT);
        assert_eq!(map.locked, CMD | SHIFT);
        assert_eq!(map.selected.as_deref(), Some("A"));
        assert!(!input.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. })), "The shortcut must select the map instead of activating an egui command");
        input.events = vec![key_event(egui::Key::A, modifiers, false), egui::Event::ModifiersChanged(egui::Modifiers::NONE)];
        map.handle_input(&mut input, 0);
        assert_eq!(map.locked, CMD | SHIFT);
        assert_eq!(map.selected.as_deref(), Some("A"));

        let mut list: Vec<_> = (0..17).map(|i| Shortcut::new(&format!("App {i:02}"), "Selected action", Trigger::combo(CMD | SHIFT, "A"), Scope::App, Origin::MenuFile)).collect();
        list.extend([
            Shortcut::new("Other key", "Wrong key", Trigger::combo(CMD | SHIFT, "B"), Scope::App, Origin::MenuFile),
            Shortcut::new("Other modifiers", "Wrong modifiers", Trigger::combo(CMD, "A"), Scope::App, Origin::MenuFile),
        ]);
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1240.0, 620.0))), ..Default::default() }, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    map.show(ui, &list, &[], &mut Default::default(), &Default::default());
                });
            });
            output.textures_delta.clear();
            text_rect(&output, "Apps using ⇧⌘A");
            text_rect(&output, "17 shortcuts · 17 apps");
        }
    }

    #[test]
    fn another_physical_shortcut_replaces_the_previous_modifiers() {
        let mut map = KeyboardMap { locked: CMD | SHIFT, selected: Some("A".into()), ..Default::default() };
        let mut input = egui::RawInput { events: vec![key_event(egui::Key::B, egui::Modifiers::COMMAND, true)], ..Default::default() };
        map.handle_input(&mut input, CMD);
        assert_eq!(map.locked, CMD);
        assert_eq!(map.selected.as_deref(), Some("B"));
        // Clicked modifiers can also be completed by a plain character key.
        input.events = vec![key_event(egui::Key::A, egui::Modifiers::NONE, true)];
        map.handle_input(&mut input, 0);
        assert_eq!(map.locked, CMD);
        assert_eq!(map.selected.as_deref(), Some("A"));
    }

    #[test]
    fn unmodified_navigation_keeps_working_and_escape_clears_the_key() {
        let mut map = KeyboardMap { locked: CMD | SHIFT, selected: Some("A".into()), ..Default::default() };
        let mut input = egui::RawInput { events: vec![key_event(egui::Key::Tab, egui::Modifiers::NONE, true), key_event(egui::Key::Space, egui::Modifiers::NONE, true)], ..Default::default() };
        map.handle_input(&mut input, 0);
        assert_eq!(input.events.len(), 2);
        assert_eq!(map.selected.as_deref(), Some("A"));
        input.events = vec![key_event(egui::Key::Escape, egui::Modifiers::NONE, true)];
        map.handle_input(&mut input, 0);
        assert_eq!(map.selected, None);
        assert_eq!(map.locked, CMD | SHIFT);
    }

    #[test]
    fn modifiers_scroll_the_shortcuts_without_moving_the_keyboard_or_zooming() {
        for size in [egui::vec2(1240.0, 620.0), egui::vec2(900.0, 360.0)] {
            for modifiers in [egui::Modifiers::COMMAND, egui::Modifiers::SHIFT, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT] {
                let ctx = egui::Context::default();
                let mut map = KeyboardMap { locked: CMD | SHIFT, ..Default::default() };
                let list: Vec<_> = (0..100).map(|i| Shortcut::new("Test app", &format!("Action {i:03}"), Trigger::combo(CMD | SHIFT, "A"), Scope::App, Origin::MenuFile)).collect();
                let mut time = 0.0;
                let mut draw = |mut events: Vec<egui::Event>| {
                    time += 0.02;
                    events.insert(0, egui::Event::ModifiersChanged(modifiers));
                    let mut input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)), time: Some(time), events, ..Default::default() };
                    prepare_scroll(&mut input);
                    let mut output = ctx.run_ui(input, |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            map.show(ui, &list, &[], &mut Default::default(), &Default::default());
                        });
                    });
                    output.textures_delta.clear();
                    output
                };
                // Let egui establish hover regions before delivering a wheel event.
                draw(vec![]);
                let before = draw(vec![]);
                let keyboard = text_rect(&before, "esc");
                let action = text_rect(&before, "Action 005");
                let title = text_rect(&before, "All ⇧⌘ shortcuts");
                assert!(action.left() > keyboard.right());
                assert!(action.right() <= size.x, "Shortcut row exceeded the pane");
                assert!(text_rect(&before, "space").bottom() <= size.y, "The keyboard bottom row left the viewport");
                let pointer = action.center();
                draw(vec![egui::Event::PointerMoved(pointer)]);
                draw(vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    // macOS can swap axes before egui sees Shift-scroll.
                    delta: if modifiers.shift { egui::vec2(-60.0, 0.0) } else { egui::vec2(0.0, -60.0) },
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers,
                }]);
                let after = draw(vec![]);
                assert!(text_rect(&after, "Action 005").top() < action.top(), "The shortcut list did not scroll with {modifiers:?}");
                assert_eq!(text_rect(&after, "esc"), keyboard, "Scrolling moved the keyboard");
                assert_eq!(text_rect(&after, "All ⇧⌘ shortcuts"), title, "Scrolling moved the list heading");
                ctx.input(|i| {
                    assert_eq!(i.zoom_delta(), 1.0);
                    assert_eq!(i.modifiers, modifiers, "Modifier selection must remain available");
                });
                assert_eq!(ctx.zoom_factor(), 1.0);
            }
        }
    }
}
