//! The welcome page (first start) and the Settings window.

use crate::settings::{self, PanelKey};
use crate::theme::{card, dot, medium, overline, segmented, semi, th};
use crate::updates::{Checker, Status as UpdateStatus};
use crate::usage;
use eframe::egui::{self, RichText};

/// One sentence that says what the app is for.
pub const PURPOSE: &str = "Use the keyboard more and the mouse less. Keyclash shows each shortcut on this Mac, finds the shortcuts that clash, and shows which shortcuts you use.";

const COUNT_LABEL: &str = "Count my shortcut presses";
const COUNT_DETAIL: &str = "Keyclash then counts each key press with ⌘, ⌃, ⌥ or 🌐, per app and per day, so you see which shortcuts you use. It does not record what you type.";

/// The welcome page. `count` is the tick box for the use counter. True when "Start" was clicked.
pub fn show(ui: &mut egui::Ui, count: &mut bool, logo: Option<&egui::TextureHandle>) -> bool {
    let t = th();
    let mut start = false;
    egui::ScrollArea::vertical().id_salt("welcome").auto_shrink([false, false]).show(ui, |ui| {
        let width = ui.available_width().min(640.0);
        let pad = ((ui.available_width() - width) / 2.0).max(0.0);
        ui.horizontal(|ui| {
            ui.add_space(pad);
            ui.vertical(|ui| {
                ui.set_width(width);
                ui.add_space(26.0);
                ui.horizontal(|ui| {
                    if let Some(tex) = logo {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(64.0, 64.0), egui::Sense::hover());
                        egui::Image::new(tex).paint_at(ui, r);
                        ui.add_space(8.0);
                    }
                    ui.vertical(|ui| {
                        ui.add_space(6.0);
                        ui.label(semi("Welcome to Keyclash").size(26.0).color(t.text));
                        ui.label(RichText::new(format!("Version {}", crate::bundle::VERSION)).size(12.5).color(t.muted));
                    });
                });
                ui.add_space(10.0);
                ui.label(RichText::new(PURPOSE).size(15.0).color(t.text));
                ui.add_space(12.0);
                for (title, text) in [
                    (
                        "What Keyclash reads",
                        "The keyboard shortcut settings of macOS and of your apps, the config files of key tools, and the menus of your apps. Keyclash reads them. It does not change them.",
                    ),
                    (
                        "The permission it asks for",
                        "Accessibility. With it, Keyclash reads the exact menus of open apps, and it knows when you press the panel key two times. Without it, Keyclash still shows the shortcuts that it finds in files.",
                    ),
                    (
                        "What it keeps",
                        "Your shortcut data and settings stay on this Mac. Keyclash checks GitHub for new versions. You can turn off automatic checks in Settings.",
                    ),
                ] {
                    card().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(overline(title));
                        ui.label(RichText::new(text).size(13.5).color(t.text2));
                    });
                    ui.add_space(6.0);
                }
                card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(overline("Your choice"));
                    ui.checkbox(count, medium(COUNT_LABEL).size(14.0).color(t.text));
                    ui.label(RichText::new(format!("{COUNT_DETAIL} The counter is off until you turn it on. You can change this in Settings.")).size(13.0).color(t.text2));
                });
                ui.add_space(14.0);
                let button = egui::Button::new(semi("Start").size(15.0).color(t.on_accent)).fill(t.accent).min_size(egui::vec2(120.0, 34.0));
                if ui.add(button).clicked() {
                    start = true;
                }
                ui.add_space(24.0);
            });
        });
    });
    start
}

/// What the main window must do after a change in Settings.
#[derive(PartialEq, Eq, Debug)]
pub enum Change {
    Nothing,
    /// The panel key changed: read the shortcuts again, because the list holds this app's own hotkey.
    ScanAgain,
    ShowWelcome,
    CheckForUpdates,
}

pub fn settings_content(ui: &mut egui::Ui, save_appearance: bool, updates: &Checker) -> Change {
    let t = th();
    let mut change = Change::Nothing;
    egui::ScrollArea::vertical().id_salt("settings").auto_shrink([false, false]).show(ui, |ui| {
        let s = settings::get();

        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(semi("Appearance").size(15.0).color(t.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut pref = crate::theme::pref();
                    let choices = [
                        (crate::theme::Pref::System, "Auto", "Follow the macOS appearance"),
                        (crate::theme::Pref::Light, "Light", ""),
                        (crate::theme::Pref::Dark, "Dark", ""),
                    ];
                    if segmented(ui, &mut pref, &choices) {
                        crate::theme::set_pref(ui.ctx(), pref, save_appearance);
                    }
                });
            });
        });
        ui.add_space(8.0);

        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(semi("Updates").size(15.0).color(t.text));
            let mut automatic = s.automatic_update_checks;
            if ui.checkbox(&mut automatic, "Check for updates automatically").changed() {
                settings::update(|s| s.automatic_update_checks = automatic);
                ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
            }
            ui.label(RichText::new("Update checks contact GitHub. Your shortcut data stays on this Mac.").size(13.0).color(t.text2));
            ui.horizontal_wrapped(|ui| {
                let checking = matches!(updates.status, UpdateStatus::Checking);
                if ui.add_enabled(!checking, egui::Button::new("Check for Updates")).clicked() {
                    change = Change::CheckForUpdates;
                }
                let status = match updates.status {
                    UpdateStatus::Idle => None,
                    UpdateStatus::Checking => Some("Checking…".to_string()),
                    UpdateStatus::Current => Some("You’re up to date.".to_string()),
                    UpdateStatus::Available(version) => Some(format!("Keyclash {version} is available.")),
                    UpdateStatus::Preparing(version) => Some(format!("The Keyclash {version} download is pending.")),
                    UpdateStatus::Failed => Some("Couldn’t check GitHub. Try again.".to_string()),
                };
                if let Some(status) = status {
                    let label = ui.label(RichText::new(status).size(13.0).color(t.text2));
                    ui.ctx().accesskit_node_builder(label.id, |node| {
                        node.set_live(egui::accesskit::Live::Polite);
                    });
                }
                if let Some(version) = updates.available {
                    ui.hyperlink_to("View release", version.release_url());
                }
            });
        });
        ui.add_space(8.0);

        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(semi(crate::panel::NAME).size(15.0).color(t.text));
            ui.label(RichText::new("Press this key two times in any app to see the shortcuts of that app. A quick double press opens Peek with the keyboard: type to filter, use the arrow keys, and press Return to run a shortcut. Hold the second press to see Peek only while you hold.").size(13.0).color(t.text2));
            ui.add_space(6.0);
            let mut key = s.panel_key;
            let options = [(PanelKey::Command, "⌘ ⌘", "Command two times"), (PanelKey::Option, "⌥ ⌥", "Option two times"), (PanelKey::Control, "⌃ ⌃", "Control two times"), (PanelKey::Off, "Off", "No key opens the panel")];
            if segmented(ui, &mut key, &options) && key != s.panel_key {
                settings::update(|s| s.panel_key = key);
                change = Change::ScanAgain;
            }
            ui.add_space(2.0);
            ui.label(RichText::new("If another tool uses the same key, the Conflicts tab shows it under \"Same key by itself\".").size(12.5).color(t.muted));
            ui.add_space(8.0);
            ui.horizontal(|ui| crate::panel::mode_picker(ui));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let size = settings::get().panel_font;
                let (lo, hi) = settings::PANEL_FONT_RANGE;
                ui.label(RichText::new("Text size").color(t.muted).size(12.0));
                if ui.add_enabled(size > lo, egui::Button::new("−")).on_hover_text("Smaller text (⌘ − in Peek)").clicked() {
                    settings::update(|s| s.panel_font = (size - 1.0).max(lo));
                }
                ui.label(RichText::new(format!("{size:.0} pt")).color(t.text));
                if ui.add_enabled(size < hi, egui::Button::new("+")).on_hover_text("Larger text (⌘ + in Peek)").clicked() {
                    settings::update(|s| s.panel_font = (size + 1.0).min(hi));
                }
            });
        });
        ui.add_space(8.0);

        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(semi("Use counter").size(15.0).color(t.text));
            let mut on = s.count_keys;
            if ui.checkbox(&mut on, medium(COUNT_LABEL).size(14.0).color(t.text)).changed() {
                usage::set_paused(!on);
            }
            ui.label(RichText::new(format!("{COUNT_DETAIL} The counts are on the Use tab and on the keyboard map.")).size(13.0).color(t.text2));
            if on && !usage::counting() {
                ui.horizontal(|ui| {
                    dot(ui, t.warn);
                    ui.label(RichText::new("This copy does not count. Only the Keyclash app counts.").size(12.5).color(t.text2));
                });
            }
        });
        ui.add_space(8.0);

        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(semi("Your data").size(15.0).color(t.text));
            let dir = crate::sources::data_dir();
            ui.label(RichText::new("Your settings, stars, counts and saved menus stay in one folder on this Mac.").size(13.0).color(t.text2));
            ui.add(egui::Label::new(RichText::new(dir.display().to_string()).size(12.5).color(t.muted).monospace()).wrap());
            if ui.button("Show in Finder").clicked() {
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::process::Command::new("/usr/bin/open").arg(&dir).spawn();
            }
        });
        ui.add_space(8.0);

        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(semi("About").size(15.0).color(t.text));
            ui.label(RichText::new(format!("Keyclash {}", crate::bundle::VERSION)).size(13.5).color(t.text));
            ui.label(RichText::new(PURPOSE).size(13.0).color(t.text2));
            ui.add_space(4.0);
            if ui.add(egui::Button::new(RichText::new("Show the welcome page again").size(12.5)).small()).clicked() {
                change = Change::ShowWelcome;
            }
        });
    });
    change
}
