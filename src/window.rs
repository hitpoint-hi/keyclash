//! Native window placement in desktop coordinates.

use eframe::egui::{self, Pos2, Rect, Vec2};

pub fn peek_builder(rect: Rect, wants_focus: bool) -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(format!("Keyclash — {}", crate::panel::NAME))
        .with_inner_size(rect.size())
        .with_position(rect.min)
        .with_active(wants_focus)
        .with_decorations(false)
        .with_always_on_top()
        .with_resizable(true)
}

pub fn work_areas(ctx: &egui::Context) -> Vec<Rect> {
    #[cfg(target_os = "macos")]
    if let Some(mtm) = objc2::MainThreadMarker::new() {
        let screens = objc2_app_kit::NSScreen::screens(mtm);
        if let Some(primary) = screens.firstObject() {
            let frame = primary.frame();
            let top = frame.origin.y + frame.size.height;
            let areas: Vec<_> = screens.iter().map(|screen| {
                let frame = screen.visibleFrame();
                Rect::from_min_size(
                    egui::pos2(frame.origin.x as f32, (top - frame.origin.y - frame.size.height) as f32),
                    egui::vec2(frame.size.width as f32, frame.size.height as f32),
                ) / ctx.zoom_factor()
            }).filter(|r| r.is_finite() && r.is_positive()).collect();
            if !areas.is_empty() {
                return areas;
            }
        }
    }
    let size = ctx.input(|i| i.viewport().monitor_size).unwrap_or(egui::vec2(1512.0, 982.0));
    vec![Rect::from_min_size(Pos2::ZERO, size)]
}

pub fn peek_rect(saved: Option<Rect>, size_hint: Vec2, screens: &[Rect], anchor: Pos2) -> Rect {
    let fallback = screens.iter().find(|r| r.contains(anchor)).copied().unwrap_or(screens[0]);
    let saved = saved.filter(|r| r.is_finite() && r.is_positive());
    let screen = saved.and_then(|saved| {
        screens.iter().copied()
            .filter(|screen| screen.intersects(saved))
            .max_by(|a, b| a.intersect(saved).area().total_cmp(&b.intersect(saved).area()))
    }).unwrap_or(fallback);
    let size = match saved {
        Some(r) => r.size().min(screen.size()),
        None => size_hint.min(screen.size() * egui::vec2(crate::panel::WIDTH_SHARE, crate::panel::HEIGHT_SHARE)).round(),
    };
    let centered = screen.min + egui::vec2((screen.width() - size.x) / 2.0, (screen.height() - size.y) / 2.5);
    let pos = saved.filter(|r| screen.intersects(*r)).map(|r| r.min).unwrap_or(centered);
    let pos = egui::pos2(
        pos.x.clamp(screen.left(), screen.right() - size.x),
        pos.y.clamp(screen.top(), screen.bottom() - size.y),
    );
    Rect::from_min_size(pos, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_peek_never_requests_initial_activation() {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 420.0));
        assert_eq!(peek_builder(rect, false).active, Some(false), "Held Peek must open without activating Keyclash");
        assert_eq!(peek_builder(rect, true).active, Some(true), "Quick-tap Peek must accept keyboard input");
    }

    #[test]
    fn peek_returns_to_the_laptop_after_disconnect() {
        let laptop = Rect::from_min_size(egui::pos2(0.0, 38.0), egui::vec2(1512.0, 920.0));
        let saved = Rect::from_min_size(egui::pos2(1950.0, 55.0), egui::vec2(1412.0, 412.0));
        let restored = peek_rect(Some(saved), saved.size(), &[laptop], laptop.center());
        assert!(laptop.contains_rect(restored), "Peek must be visible on the laptop: {restored:?}");
    }

    #[test]
    fn a_connected_external_display_keeps_the_saved_placement() {
        let laptop = Rect::from_min_size(egui::pos2(0.0, 38.0), egui::vec2(1512.0, 920.0));
        let external = Rect::from_min_size(egui::pos2(1512.0, 38.0), egui::vec2(2560.0, 1400.0));
        let saved = Rect::from_min_size(egui::pos2(1950.0, 55.0), egui::vec2(1412.0, 412.0));
        assert_eq!(peek_rect(Some(saved), saved.size(), &[laptop, external], laptop.center()), saved);
    }

    #[test]
    fn oversized_and_partial_windows_fit_the_visible_area() {
        let laptop = Rect::from_min_size(egui::pos2(0.0, 38.0), egui::vec2(1280.0, 720.0));
        for (pos, size) in [(egui::pos2(1950.0, 55.0), egui::vec2(1800.0, 1000.0)), (egui::pos2(1100.0, 700.0), egui::vec2(700.0, 500.0))] {
            let saved = Rect::from_min_size(pos, size);
            assert!(laptop.contains_rect(peek_rect(Some(saved), size, &[laptop], laptop.center())));
        }
    }

    #[test]
    fn new_panels_center_on_displays_left_of_or_above_the_primary() {
        let primary = Rect::from_min_size(egui::pos2(0.0, 38.0), egui::vec2(1512.0, 920.0));
        for origin in [egui::pos2(-1920.0, 0.0), egui::pos2(0.0, -1080.0)] {
            let external = Rect::from_min_size(origin, egui::vec2(1920.0, 1080.0));
            let rect = peek_rect(None, egui::vec2(900.0, 500.0), &[primary, external], external.center());
            assert!(external.contains_rect(rect));
            assert_eq!(peek_rect(Some(rect), rect.size(), &[primary, external], primary.center()), rect);
        }
    }
}
