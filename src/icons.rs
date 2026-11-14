//! App icons next to app names. People know an app by its icon before they read its name.
//!
//! The icon comes from the `.icns` file inside the app bundle. That file is a list of pictures in
//! several sizes, and the modern sizes are PNG data, so no macOS call and no other program is needed.
//! An app with no readable icon gets a badge with its first letter.

use crate::model::AppInfo;
use crate::theme::th;
use eframe::egui;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// The side of the stored picture, in pixels. Rows draw it at 16 to 20 points.
const SIDE: u32 = 64;
/// Icons read from disk per frame, so a long list does not stop the window.
const LOADS_PER_FRAME: u32 = 3;

/// `.icns` entry types that can hold PNG data, with their pixel size. Smallest size that is
/// still sharp comes first.
const ENTRIES: [(&[u8; 4], u32); 10] =
    [(b"ic12", 64), (b"icp6", 64), (b"ic07", 128), (b"ic13", 256), (b"ic08", 256), (b"ic14", 512), (b"ic09", 512), (b"ic10", 1024), (b"ic11", 32), (b"icp5", 32)];

/// The PNG data of the best entry in an `.icns` file.
pub fn png_in_icns(data: &[u8]) -> Option<&[u8]> {
    if data.len() < 8 || &data[..4] != b"icns" {
        return None;
    }
    let mut found: HashMap<[u8; 4], &[u8]> = HashMap::new();
    let mut at = 8;
    while at + 8 <= data.len() {
        let kind: [u8; 4] = data[at..at + 4].try_into().ok()?;
        let len = u32::from_be_bytes(data[at + 4..at + 8].try_into().ok()?) as usize;
        if len < 8 || at + len > data.len() {
            break;
        }
        let body = &data[at + 8..at + len];
        // Old files keep some sizes as JPEG 2000 or raw pixels: only PNG data is used.
        if body.starts_with(b"\x89PNG") {
            found.insert(kind, body);
        }
        at += len;
    }
    ENTRIES.iter().find_map(|(kind, _)| found.get(*kind).copied())
}

/// The icon file of an app bundle: the one its Info.plist names, else the first `.icns` in Resources.
pub fn icon_file(app: &Path) -> Option<PathBuf> {
    let resources = app.join("Contents/Resources");
    let named = plist::Value::from_file(app.join("Contents/Info.plist"))
        .ok()
        .and_then(|v| v.as_dictionary().and_then(|d| d.get("CFBundleIconFile")).and_then(|v| v.as_string()).map(String::from));
    if let Some(name) = named {
        for file in [name.clone(), format!("{name}.icns")] {
            let p = resources.join(file);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let mut all: Vec<PathBuf> = std::fs::read_dir(&resources).ok()?.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "icns")).collect();
    all.sort();
    all.into_iter().next()
}

/// The icon of an app as a square picture.
fn load(app: &Path) -> Option<egui::ColorImage> {
    let data = std::fs::read(icon_file(app)?).ok()?;
    let img = image::load_from_memory(png_in_icns(&data)?).ok()?.to_rgba8();
    let img = if img.width() > SIDE { image::imageops::resize(&img, SIDE, SIDE, image::imageops::FilterType::Triangle) } else { img };
    let size = [img.width() as usize, img.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, &img.into_raw()))
}

#[derive(Default)]
struct State {
    /// App id (lower case) and app name (lower case) to the app bundle path.
    by_id: HashMap<String, String>,
    by_name: HashMap<String, String>,
    /// App bundle path to its picture. None: the app has no readable icon.
    pictures: HashMap<String, Option<egui::TextureHandle>>,
    frame: u64,
    loads: u32,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(Default::default)
}

/// Tell the icons where each app is. Call after a scan.
pub fn set_apps(apps: &[AppInfo]) {
    let Ok(mut s) = state().lock() else { return };
    s.by_id.clear();
    s.by_name.clear();
    for a in apps {
        if let Some(b) = &a.bundle {
            s.by_id.entry(b.to_lowercase()).or_insert_with(|| a.path.clone());
        }
        s.by_name.entry(a.name.to_lowercase()).or_insert_with(|| a.path.clone());
    }
    // macOS's own shortcuts are set in System Settings.
    s.by_id.insert(crate::sources::MACOS.to_string(), "/System/Applications/System Settings.app".to_string());
}

/// Tell the icons where one app is, when a scan did not list it (an app that started later).
pub fn register(bundle: &str, path: &str) {
    if let Ok(mut s) = state().lock() {
        s.by_id.entry(bundle.to_lowercase()).or_insert_with(|| path.to_string());
    }
}

/// The picture for an app, by app id or by name. None while it is not read yet, and for an app with no icon.
fn picture(ctx: &egui::Context, id: Option<&str>, name: &str) -> Option<egui::TextureHandle> {
    let mut s = state().lock().ok()?;
    let path = id.and_then(|b| s.by_id.get(&b.to_lowercase())).or_else(|| s.by_name.get(&name.to_lowercase()))?.clone();
    if let Some(known) = s.pictures.get(&path) {
        return known.clone();
    }
    let frame = ctx.cumulative_frame_nr();
    if s.frame != frame {
        s.frame = frame;
        s.loads = 0;
    }
    if s.loads >= LOADS_PER_FRAME {
        // Read it in a later frame.
        ctx.request_repaint();
        return None;
    }
    s.loads += 1;
    let tex = load(Path::new(&path)).map(|img| ctx.load_texture(format!("icon:{path}"), img, egui::TextureOptions::LINEAR));
    s.pictures.insert(path, tex.clone());
    tex
}

/// Draw the icon of an app in `rect`. An app with no icon gets a badge with its first letter.
pub fn paint(ui: &egui::Ui, rect: egui::Rect, id: Option<&str>, name: &str) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    match picture(ui.ctx(), id, name) {
        Some(tex) => {
            egui::Image::new(&tex).paint_at(ui, rect);
        }
        None => {
            let t = th();
            let badge = rect.shrink(rect.width() * 0.06);
            ui.painter().rect_filled(badge, badge.width() * 0.26, t.raised);
            let letter = name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_default();
            ui.painter().text(badge.center(), egui::Align2::CENTER_CENTER, letter, egui::FontId::proportional(rect.height() * 0.58), t.muted);
        }
    }
}

/// Take room for an icon of `size` points in the row and draw it.
pub fn show(ui: &mut egui::Ui, id: Option<&str>, name: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    paint(ui, rect, id, name);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut e = kind.to_vec();
        e.extend(((body.len() + 8) as u32).to_be_bytes());
        e.extend(body);
        e
    }

    #[test]
    fn the_best_png_in_an_icns_file_is_found() {
        let png_128 = b"\x89PNG-128".to_vec();
        let png_1024 = b"\x89PNG-1024".to_vec();
        let mut body = entry(b"ic10", &png_1024);
        body.extend(entry(b"il32", b"raw pixels"));
        body.extend(entry(b"ic08", b"\x00\x00\x00\x0cjP  jpeg2000"));
        body.extend(entry(b"ic07", &png_128));
        let mut file = b"icns".to_vec();
        file.extend(((body.len() + 8) as u32).to_be_bytes());
        file.extend(&body);
        // 128 pixels is the smallest PNG. The JPEG 2000 and raw entries are not used.
        assert_eq!(png_in_icns(&file), Some(png_128.as_slice()));
        assert_eq!(png_in_icns(b"not an icon file"), None);
        // A file that stops in the middle of an entry gives what was read before it.
        assert_eq!(png_in_icns(&file[..file.len() - 3]), Some(png_1024.as_slice()));
    }

    #[test]
    fn an_app_on_this_mac_has_an_icon() {
        // Finder is on each Mac.
        let finder = Path::new("/System/Library/CoreServices/Finder.app");
        if finder.exists() {
            let img = load(finder).expect("Finder icon");
            assert_eq!(img.size[0], img.size[1]);
            assert!(img.size[0] as u32 <= SIDE && img.size[0] >= 32);
        }
    }
}
