//! Colors, fonts and shared widgets for light and dark mode.
//! Every screen reads its colors from `th()`, so one switch changes the whole app.

use crate::conflicts::Kind;
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, StrokeKind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

pub struct Theme {
    pub dark: bool,
    /// Window background.
    pub bg: Color32,
    /// Header and floating panels.
    pub surface: Color32,
    /// Cards on the background.
    pub card: Color32,
    /// Buttons, hovered rows, segmented controls.
    pub raised: Color32,
    /// Text boxes.
    pub field: Color32,
    pub line: Color32,
    pub line_strong: Color32,
    pub text: Color32,
    pub text2: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub keycap: Color32,
    pub keycap_line: Color32,
    pub keycap_edge: Color32,
    // Modifier symbols. Each hue is tuned to stay readable on its background.
    pub cmd: Color32,
    pub opt: Color32,
    pub shift: Color32,
    pub ctrl: Color32,
    pub fnk: Color32,
    pub good: Color32,
    pub warn: Color32,
    pub star: Color32,
    pub clash: Color32,
    pub blocks: Color32,
    pub same: Color32,
    pub inside: Color32,
    pub shared: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// The dark theme uses the dark sections of keyclash.app, which come from the app icon
/// (`assets/make_icon.swift`): the navy tile, the blue and red keys, and the yellow spark.
pub static DARK: Theme = Theme {
    dark: true,
    // Navy surfaces from the icon tile (#0F1219 to #2B3145), darkest at the back.
    bg: rgb(0x0F1219),
    surface: rgb(0x151821),
    card: rgb(0x171B25),
    raised: rgb(0x1F2431),
    field: rgb(0x0B0D13),
    line: rgb(0x262B38),
    line_strong: rgb(0x3A4152),
    text: rgb(0xEEF0F4),
    text2: rgb(0xC3C8D2),
    muted: rgb(0x8E95A3),
    faint: rgb(0x626A7D),
    // The icon's blue key, a shade between its two stops (#6DB4FF and #2B74E0).
    // Light enough for 4.5:1 on its own badge tint and 3:1 for key outlines on cards.
    accent: rgb(0x5CA3FA),
    on_accent: rgb(0x08111F),
    keycap: rgb(0x262B38),
    keycap_line: rgb(0x4F576A),
    keycap_edge: rgb(0x07090E),
    cmd: rgb(0x4CC2FF),
    // The icon's spark yellow.
    opt: rgb(0xFFC94A),
    shift: rgb(0x4ADE80),
    ctrl: rgb(0xFF7AC8),
    fnk: rgb(0xC58AF9),
    good: rgb(0x4ADE80),
    warn: rgb(0xFFC94A),
    star: rgb(0xFFC94A),
    // A shade within the icon's red-key gradient (#FF8C7A to #E83F4D), light enough for 4.5:1 on its badge tint.
    clash: rgb(0xFF5C61),
    // The icon's spark orange.
    blocks: rgb(0xFF9A2E),
    same: rgb(0xEDC94A),
    inside: rgb(0xA896FF),
    shared: rgb(0x8E95A3),
};

/// The light theme uses the keyclash.app palette, which comes from the app icon
/// (`assets/make_icon.swift`): paper, navy ink, the blue key and the red key.
pub static LIGHT: Theme = Theme {
    dark: false,
    bg: rgb(0xF4F6FA),
    surface: rgb(0xFFFFFF),
    card: rgb(0xFFFFFF),
    raised: rgb(0xEAEDF3),
    field: rgb(0xFFFFFF),
    line: rgb(0xDFE3EA),
    line_strong: rgb(0xC6CBD5),
    text: rgb(0x151821),
    // The site's secondary (#4B5263), a step darker so it keeps 7:1 on raised controls.
    text2: rgb(0x464D5E),
    muted: rgb(0x5F6877),
    faint: rgb(0x949CAA),
    // The icon's blue key (#2B74E0), darkened for 4.5:1 on cards and under white text.
    accent: rgb(0x1F63D0),
    on_accent: rgb(0xFFFFFF),
    keycap: rgb(0xFAFBFC),
    keycap_line: rgb(0xC6CBD5),
    keycap_edge: rgb(0xB3B9C5),
    cmd: rgb(0x0B72CC),
    opt: rgb(0xA26200),
    shift: rgb(0x16803A),
    ctrl: rgb(0xB8287A),
    fnk: rgb(0x8338C2),
    good: rgb(0x16803A),
    warn: rgb(0xA26200),
    star: rgb(0xD99A00),
    // The icon's red key (#E83F4D) at the same hue, darkened for 4.5:1 on its own badge tint
    // and kept apart from the Control pink.
    clash: rgb(0xC21F2B),
    blocks: rgb(0xAD4B00),
    same: rgb(0x7D5D00),
    inside: rgb(0x5E43D6),
    shared: rgb(0x5F6877),
};

static IS_DARK: AtomicBool = AtomicBool::new(true);

/// The theme that is on screen now.
pub fn th() -> &'static Theme {
    if IS_DARK.load(Ordering::Relaxed) { &DARK } else { &LIGHT }
}

/// Blend `b` into `a`. The result is opaque, so it looks the same on any background.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

impl Theme {
    /// A soft fill of color `c` for badges, callouts and keys.
    pub fn tint(&self, c: Color32, t: f32) -> Color32 {
        mix(self.card, c, t)
    }

    pub fn kind(&self, k: Kind) -> Color32 {
        match k {
            Kind::Clash => self.clash,
            Kind::Blocks => self.blocks,
            Kind::SameModifier => self.same,
            Kind::InsideApp => self.inside,
            Kind::Shared => self.shared,
        }
    }

    /// The color of a modifier symbol; plain keys use the text color.
    pub fn sym(&self, s: &str) -> Color32 {
        if s.contains('⌘') {
            self.cmd
        } else if s.contains('⌥') {
            self.opt
        } else if s.contains('⇧') {
            self.shift
        } else if s.contains('⌃') {
            self.ctrl
        } else if s.contains("fn") || s.contains(crate::keys::FN_SYMBOL) {
            self.fnk
        } else {
            self.text
        }
    }
}

// ------------------------------------------------------------ preference

/// Which theme the user picked. `System` follows macOS.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pref {
    System = 0,
    Light = 1,
    Dark = 2,
}

static PREF: AtomicU8 = AtomicU8::new(0);

fn pref_path() -> std::path::PathBuf {
    crate::sources::data_dir().join("theme")
}

pub fn pref() -> Pref {
    match PREF.load(Ordering::Relaxed) {
        1 => Pref::Light,
        2 => Pref::Dark,
        _ => Pref::System,
    }
}

pub fn parse_pref(s: &str) -> Option<Pref> {
    Some(match s.trim().to_lowercase().as_str() {
        "light" => Pref::Light,
        "dark" => Pref::Dark,
        "system" | "auto" => Pref::System,
        _ => return None,
    })
}

/// Switch the theme. `save` keeps the choice for the next start.
pub fn set_pref(ctx: &egui::Context, p: Pref, save: bool) {
    PREF.store(p as u8, Ordering::Relaxed);
    ctx.set_theme(match p {
        Pref::System => egui::ThemePreference::System,
        Pref::Light => egui::ThemePreference::Light,
        Pref::Dark => egui::ThemePreference::Dark,
    });
    sync(ctx);
    if save {
        let path = pref_path();
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let name = match p {
            Pref::System => "system",
            Pref::Light => "light",
            Pref::Dark => "dark",
        };
        let _ = std::fs::write(path, name);
    }
}

/// Read which theme egui uses this frame. Call it at the start of every frame.
pub fn sync(ctx: &egui::Context) {
    IS_DARK.store(ctx.theme() == egui::Theme::Dark, Ordering::Relaxed);
}

// ------------------------------------------------------------ setup

/// Fonts, colors and spacing. `forced` (from `--theme`) wins over the saved choice and is not saved.
pub fn setup(ctx: &egui::Context, forced: Option<Pref>) {
    let mut fonts = egui::FontDefinitions::default();
    // The macOS interface font first: it has weights and the ⌘ ⌥ ⇧ ⌃ symbols.
    for (name, path, fam) in [
        ("sf", "/System/Library/Fonts/SFNS.ttf", egui::FontFamily::Proportional),
        ("sf-mono", "/System/Library/Fonts/SFNSMono.ttf", egui::FontFamily::Monospace),
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            let mut data = egui::FontData::from_owned(bytes);
            // Use the small-text optical size: the default of this file is drawn for headlines.
            if let Some(axis) = data.variation_axes().iter().find(|a| a.tag.to_string() == "opsz") {
                data.tweak.coords.push("opsz", axis.range.min.max(17.0).min(axis.range.max));
            }
            fonts.font_data.insert(name.into(), Arc::new(data));
            fonts.families.entry(fam).or_default().insert(0, name.into());
        }
    }
    // Fallbacks for symbols the interface font lacks (⎋ ⌫ ⇥ …).
    for (name, path) in [
        ("symbols", "/System/Library/Fonts/Apple Symbols.ttf"),
        ("unicode", "/System/Library/Fonts/Supplemental/Arial Unicode.ttf"),
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(name.into(), Arc::new(egui::FontData::from_owned(bytes)));
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(fam).or_default().push(name.into());
            }
        }
    }
    ctx.set_fonts(fonts);
    ctx.set_visuals_of(egui::Theme::Dark, visuals(&DARK));
    ctx.set_visuals_of(egui::Theme::Light, visuals(&LIGHT));
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(11.0, 5.0);
        s.spacing.interact_size.y = 26.0;
        for (ts, size) in [
            (egui::TextStyle::Body, 13.5),
            (egui::TextStyle::Button, 13.5),
            (egui::TextStyle::Heading, 20.0),
            (egui::TextStyle::Monospace, 13.0),
            (egui::TextStyle::Small, 11.5),
        ] {
            if let Some(f) = s.text_styles.get_mut(&ts) {
                f.size = size;
            }
        }
    });
    let saved = std::fs::read_to_string(pref_path()).ok().and_then(|s| parse_pref(&s));
    set_pref(ctx, forced.or(saved).unwrap_or(Pref::System), false);
}

fn visuals(t: &Theme) -> egui::Visuals {
    let mut v = if t.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    let r = CornerRadius::same(7);
    v.panel_fill = t.bg;
    v.window_fill = t.surface;
    v.window_stroke = Stroke::new(1.0, t.line_strong);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(9);
    v.extreme_bg_color = t.field;
    v.faint_bg_color = mix(t.card, t.text, 0.035);
    v.hyperlink_color = t.accent;
    v.warn_fg_color = t.warn;
    v.error_fg_color = t.clash;
    v.selection.bg_fill = mix(t.card, t.accent, if t.dark { 0.26 } else { 0.16 });
    v.selection.stroke = Stroke::new(1.0, if t.dark { mix(t.accent, t.text, 0.45) } else { t.accent });
    let shade = Color32::from_black_alpha(if t.dark { 110 } else { 30 });
    v.popup_shadow = egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: shade };
    v.window_shadow = egui::Shadow { offset: [0, 10], blur: 28, spread: 0, color: shade };
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = t.surface;
    w.noninteractive.weak_bg_fill = t.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, t.line);
    w.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
    w.noninteractive.corner_radius = r;
    w.inactive.bg_fill = t.raised;
    w.inactive.weak_bg_fill = t.raised;
    w.inactive.bg_stroke = Stroke::new(1.0, t.line);
    w.inactive.fg_stroke = Stroke::new(1.0, t.text2);
    w.inactive.corner_radius = r;
    let hover = mix(t.raised, t.text, 0.07);
    w.hovered.bg_fill = hover;
    w.hovered.weak_bg_fill = hover;
    w.hovered.bg_stroke = Stroke::new(1.0, t.line_strong);
    w.hovered.fg_stroke = Stroke::new(1.0, t.text);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;
    let press = mix(t.raised, t.text, 0.14);
    w.active.bg_fill = press;
    w.active.weak_bg_fill = press;
    w.active.bg_stroke = Stroke::new(1.0, t.line_strong);
    w.active.fg_stroke = Stroke::new(1.0, t.text);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;
    w.open = w.hovered;
    v
}

// ------------------------------------------------------------ text

/// Semibold text, for names and headings.
pub fn semi(text: impl Into<String>) -> RichText {
    RichText::new(text).variation("wght", 620.0)
}

/// Medium-weight text, for labels that need a little emphasis.
pub fn medium(text: impl Into<String>) -> RichText {
    RichText::new(text).variation("wght", 510.0)
}

/// A small heading above a group: capitals with wide spacing.
pub fn overline(text: &str) -> RichText {
    semi(text.to_uppercase()).size(10.5).extra_letter_spacing(0.7).color(th().muted)
}

/// "1,234".
pub fn num(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn galley(ui: &egui::Ui, text: RichText) -> Arc<egui::Galley> {
    egui::WidgetText::from(text).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body)
}

/// Text on one line, cut with "…" when it is wider than `width`.
pub fn fit(ui: &egui::Ui, text: RichText, width: f32) -> Arc<egui::Galley> {
    egui::WidgetText::from(text).into_galley(ui, Some(egui::TextWrapMode::Truncate), width.max(10.0), egui::TextStyle::Body)
}

// ------------------------------------------------------------ widgets

/// One key cap: a raised key with a centred symbol.
pub fn cap(ui: &mut egui::Ui, text: &str, color: Color32, size: f32) {
    let t = th();
    let g = galley(ui, medium(text).size(size).color(color));
    let h = (size * 1.6).round();
    let w = (g.size().x + size * 0.8).max(h).round();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h + 1.0), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let face = egui::Rect::from_min_size(rect.min, egui::vec2(w, h));
    let r = (size * 0.36).clamp(4.0, 7.0);
    let p = ui.painter();
    p.rect_filled(face.translate(egui::vec2(0.0, 1.0)), r, t.keycap_edge);
    p.rect(face, r, t.keycap, Stroke::new(1.0, t.keycap_line), StrokeKind::Inside);
    p.galley(face.center() - g.size() / 2.0, g, color);
}

/// A small rounded label, tinted with `color`.
pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    let t = th();
    let g = galley(ui, medium(text).size(11.5).color(color));
    let size = egui::vec2(g.size().x + 16.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, ui.is_enabled(), text));
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.rect(rect, 10.0, t.tint(color, 0.14), Stroke::new(1.0, t.tint(color, 0.38)), StrokeKind::Inside);
        p.galley(rect.center() - g.size() / 2.0, g, color);
    }
    resp
}

pub fn focus_ring(ui: &egui::Ui, response: &egui::Response, radius: u8) {
    if response.has_focus() {
        ui.painter().rect_stroke(response.rect, radius, Stroke::new(2.0, th().accent), StrokeKind::Inside);
    }
}

/// A filter chip with a colored dot. Returns true when clicked.
pub fn chip(ui: &mut egui::Ui, on: bool, text: &str, color: Color32) -> egui::Response {
    let t = th();
    let g = galley(ui, medium(text).size(12.5));
    let size = egui::vec2(g.size().x + 34.0, 26.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, ui.is_enabled(), on, text));
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let (fill, line, fg) = if on {
            (t.tint(color, 0.13), t.tint(color, 0.45), t.text)
        } else if resp.hovered() {
            (t.raised, t.line_strong, t.text2)
        } else {
            (Color32::TRANSPARENT, t.line, t.muted)
        };
        p.rect(rect, 13.0, fill, Stroke::new(1.0, line), StrokeKind::Inside);
        let dot = egui::pos2(rect.left() + 13.0, rect.center().y);
        if on {
            p.circle_filled(dot, 4.0, color);
        } else {
            p.circle_stroke(dot, 3.5, Stroke::new(1.5, mix(t.muted, color, 0.5)));
        }
        p.galley(egui::pos2(rect.left() + 23.0, rect.center().y - g.size().y / 2.0), g, fg);
    }
    focus_ring(ui, &resp, 13);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A tab with a line under the selected one. Returns true when clicked.
pub fn tab(ui: &mut egui::Ui, selected: bool, text: &str) -> bool {
    let t = th();
    let g = galley(ui, if selected { semi(text) } else { medium(text) }.size(13.5));
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(g.size().x + 6.0, 34.0), egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Tab, ui.is_enabled(), text));
    ui.ctx().accesskit_node_builder(resp.id, |node| node.set_selected(selected));
    if ui.is_rect_visible(rect) {
        let fg = if selected {
            t.text
        } else if resp.hovered() {
            t.text2
        } else {
            t.muted
        };
        let p = ui.painter();
        p.galley(egui::pos2(rect.center().x - g.size().x / 2.0, rect.center().y - g.size().y / 2.0 - 1.0), g, fg);
        if selected {
            let bar = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 2.5), rect.right_bottom());
            p.rect_filled(bar, CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 }, t.accent);
        }
    }
    focus_ring(ui, &resp, 4);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// A row of joined choices; one is selected. Returns true when the choice changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, current: &mut T, options: &[(T, &str, &str)]) -> bool {
    let t = th();
    let mut changed = false;
    egui::Frame::new().fill(t.raised).stroke(Stroke::new(1.0, t.line)).corner_radius(8).inner_margin(egui::Margin::same(2)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            // A right-to-left row lays its items out in reverse, so add them in reverse to keep the reading order.
            let rtl = ui.layout().main_dir() == egui::Direction::RightToLeft;
            let ordered: Vec<&(T, &str, &str)> = if rtl { options.iter().rev().collect() } else { options.iter().collect() };
            for (value, label, tip) in ordered {
                let on = *current == *value;
                let g = galley(ui, medium(*label).size(12.5));
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(g.size().x + 20.0, 22.0), egui::Sense::click());
                resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, ui.is_enabled(), on, *label));
                if ui.is_rect_visible(rect) {
                    let p = ui.painter();
                    if on {
                        let fill = if t.dark { mix(t.raised, t.text, 0.14) } else { t.surface };
                        p.rect(rect, 6.0, fill, Stroke::new(1.0, t.line_strong), StrokeKind::Inside);
                    } else if resp.hovered() {
                        p.rect_filled(rect, 6.0, mix(t.raised, t.text, 0.06));
                    }
                    let fg = if on { t.text } else { t.muted };
                    p.galley(rect.center() - g.size() / 2.0, g, fg);
                }
                let resp = if tip.is_empty() { resp } else { resp.on_hover_text(*tip) };
                focus_ring(ui, &resp, 6);
                if resp.clicked() && !on {
                    *current = *value;
                    changed = true;
                }
            }
        });
    });
    changed
}

/// The frame of a card: a filled, bordered box on the window background.
pub fn card() -> egui::Frame {
    let t = th();
    egui::Frame::new().fill(t.card).stroke(Stroke::new(1.0, t.line)).corner_radius(12).inner_margin(egui::Margin::symmetric(16, 12))
}

/// A tinted box for a notice in color `c`.
pub fn callout(c: Color32) -> egui::Frame {
    let t = th();
    egui::Frame::new()
        .fill(mix(t.surface, c, if t.dark { 0.09 } else { 0.08 }))
        .stroke(Stroke::new(1.0, mix(t.surface, c, 0.4)))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(14, 10))
}

/// A filled dot, centred on the text line.
pub fn dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 14.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_controls_expose_names_roles_and_selection() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            tab(ui, true, "Keyboard map");
            chip(ui, true, "Command modifier", DARK.cmd);
            let mut current = 1;
            segmented(ui, &mut current, &[(1, "Active tools", ""), (2, "All tools", "")]);
        });
        output.textures_delta.clear();
        let update = output.platform_output.accesskit_update.unwrap();
        let node = |label: &str| update.nodes.iter().map(|(_, n)| n).find(|n| n.label() == Some(label)).unwrap();
        assert_eq!(node("Keyboard map").role(), egui::Role::Tab);
        assert_eq!(node("Keyboard map").is_selected(), Some(true));
        assert_eq!(node("Command modifier").role(), egui::Role::CheckBox);
        assert_eq!(node("Command modifier").toggled(), Some(egui::accesskit::Toggled::True));
        assert_eq!(node("Active tools").role(), egui::Role::RadioButton);
    }

    #[test]
    fn keyboard_map_controls_belong_to_the_accessibility_tree() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut map = crate::keyboard::KeyboardMap::default();
        let mut use_view = crate::use_tab::UseView::default();
        let list = [crate::model::Shortcut::new("Test", "Action", crate::model::Trigger::combo(0, "F1"), crate::model::Scope::App, crate::model::Origin::MenuFile)];
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1240.0, 820.0))), ..Default::default() };
        for _ in 0..3 {
            let mut output = ctx.run_ui(input.clone(), |ui| {
                egui::Panel::top("header").show(ui, |ui| { tab(ui, true, "Keyboard map"); });
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
                        node.set_role(egui::Role::Group);
                        node.set_label("Shortcut content");
                    });
                    map.show(ui, &list, &[], &mut use_view, &Default::default());
                });
            });
            output.textures_delta.clear();
            let update = output.platform_output.accesskit_update.unwrap();
            let nodes: std::collections::HashMap<_, _> = update.nodes.into_iter().collect();
            fn labels<'a>(id: egui::accesskit::NodeId, nodes: &'a std::collections::HashMap<egui::accesskit::NodeId, egui::accesskit::Node>, out: &mut Vec<&'a str>) {
                let Some(n) = nodes.get(&id) else { return };
                if let Some(label) = n.label() { out.push(label); }
                for child in n.children() { labels(*child, nodes, out); }
            }
            let mut reachable = Vec::new();
            labels(update.tree.unwrap().root, &nodes, &mut reachable);
            assert!(reachable.iter().any(|s| s.contains("Command modifier")), "{reachable:?}");
            assert!(reachable.iter().any(|s| s.contains("f1: used inside an app")), "{reachable:?}");
            assert!(reachable.contains(&"Star: Action in Test"), "{reachable:?}");
        }
    }

    /// WCAG contrast ratio of two opaque colors.
    fn contrast(a: Color32, b: Color32) -> f32 {
        let lum = |c: Color32| {
            let f = |v: u8| {
                let x = v as f32 / 255.0;
                if x <= 0.03928 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn text_is_readable_in_both_themes() {
        for t in [&DARK, &LIGHT] {
            // `raised` fills buttons and hovered chips, which show text2 labels.
            for bg in [t.bg, t.surface, t.card, t.raised] {
                for (name, fg, min) in [("text", t.text, 7.0), ("text2", t.text2, 7.0), ("muted", t.muted, 4.5)] {
                    let c = contrast(fg, bg);
                    assert!(c >= min, "{name} on {bg:?} (dark={}) has contrast {c:.2}, need {min}", t.dark);
                }
            }
            // Colored text: modifier symbols on key caps, and status words on cards.
            for (name, fg) in [("cmd", t.cmd), ("opt", t.opt), ("shift", t.shift), ("ctrl", t.ctrl), ("fn", t.fnk)] {
                let c = contrast(fg, t.keycap);
                assert!(c >= 4.5, "{name} on a key cap (dark={}) has contrast {c:.2}", t.dark);
            }
            for k in [Kind::Clash, Kind::Blocks, Kind::SameModifier, Kind::InsideApp, Kind::Shared] {
                let c = contrast(t.kind(k), t.tint(t.kind(k), 0.14));
                assert!(c >= 4.5, "{k:?} badge (dark={}) has contrast {c:.2}", t.dark);
            }
            assert!(contrast(t.accent, t.card) >= 4.5, "accent on a card (dark={})", t.dark);
            // The "System-wide" tag is a pill in the accent color.
            let c = contrast(t.accent, t.tint(t.accent, 0.14));
            assert!(c >= 4.5, "System-wide badge (dark={}) has contrast {c:.2}", t.dark);
            // Red means a clash. The Control symbol must not look red, or a ⌃ row reads as a clash.
            let hue_gap = |a: Color32, b: Color32| (a.r() as i32 - b.r() as i32).abs() + (a.g() as i32 - b.g() as i32).abs() + (a.b() as i32 - b.b() as i32).abs();
            assert!(hue_gap(t.ctrl, t.clash) >= 90, "ctrl and clash colors are too close (dark={})", t.dark);
        }
    }

    #[test]
    fn thousands() {
        assert_eq!(num(7), "7");
        assert_eq!(num(1234), "1,234");
        assert_eq!(num(1234567), "1,234,567");
    }
}
