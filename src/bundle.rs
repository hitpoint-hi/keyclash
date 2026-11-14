//! Turn this program into a real Mac app ("Keyclash.app" in ~/Applications),
//! so macOS can list it under Privacy & Security › Accessibility by its own name.

use std::path::PathBuf;
use std::process::Command;

pub const BUNDLE_ID: &str = "app.keyclash.Keyclash";
/// The app id of versions before 0.1.0 was shared. An app bundle with this id is an old copy.
const OLD_BUNDLE_IDS: [&str; 1] = ["com.craftanything.shortcutatlas"];
/// The program file inside the app bundle.
const PROGRAM: &str = "keyclash";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_NAME: &str = "Keyclash";
/// Names of earlier versions. Their app bundles are removed at install, so one app has one bundle.
const OLD_APP_NAMES: [&str; 2] = ["Shortcut Atlas", "Chordbook"];

/// The app icon (Finder, Dock, Accessibility list). Drawn by `assets/make_icon.swift`.
const APP_ICON: &[u8] = include_bytes!("../assets/AppIcon.icns");
/// The same picture for the window and the Dock of a copy that is not installed.
const WINDOW_ICON_PNG: &[u8] = include_bytes!("../assets/icon-512.png");
/// The picture without the margin, for the header of the window.
pub const LOGO_PNG: &[u8] = include_bytes!("../assets/logo-192.png");

/// A PNG as (RGBA bytes, width, height).
pub fn decode_png(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some((img.into_raw(), w, h))
}

/// The icon for the window.
pub fn window_icon() -> Option<eframe::egui::IconData> {
    let (rgba, width, height) = decode_png(WINDOW_ICON_PNG)?;
    Some(eframe::egui::IconData { rgba, width, height })
}

/// True when this copy runs from inside an .app bundle.
pub fn running_as_app() -> bool {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().contains(".app/Contents/MacOS/"))
        .unwrap_or(false)
}

pub fn install_path() -> PathBuf {
    crate::sources::home().join("Applications").join(format!("{APP_NAME}.app"))
}

fn info_plist() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>{APP_NAME}</string>
    <key>CFBundleDisplayName</key><string>{APP_NAME}</string>
    <key>CFBundleIdentifier</key><string>{BUNDLE_ID}</string>
    <key>CFBundleExecutable</key><string>{PROGRAM}</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>{VERSION}</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSAppSleepDisabled</key><true/>
    <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
</dict>
</plist>
"#
    )
}

/// Build the .app from the running program in ~/Applications, and open it.
pub fn install_and_open() -> Result<PathBuf, String> {
    let app = install_path();
    make_bundle(&app)?;
    for old in OLD_APP_NAMES {
        let old_app = crate::sources::home().join("Applications").join(format!("{old}.app"));
        // Remove only a bundle that is this app: check its id first.
        let is_ours = std::fs::read_to_string(old_app.join("Contents/Info.plist")).is_ok_and(|t| OLD_BUNDLE_IDS.iter().chain([&BUNDLE_ID]).any(|id| t.contains(&format!("<string>{id}</string>"))));
        if is_ours {
            let _ = std::fs::remove_dir_all(&old_app);
        }
    }
    Command::new("/usr/bin/open").arg("-n").arg(&app).spawn().map_err(|e| format!("cannot open the app: {e}"))?;
    Ok(app)
}

/// Build the .app at `app` from the running program and sign it with an ad-hoc signature.
/// The new bundle is built and signed beside the old one, then swapped in, so a failure leaves
/// the old app working. This also holds when the running program is the old app's own file.
pub fn make_bundle(app: &std::path::Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find this program: {e}"))?;
    let name = app.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| format!("{APP_NAME}.app"));
    let sibling = |tag: &str| app.with_file_name(format!(".{name}.{tag}-{}", std::process::id()));
    let (new, old) = (sibling("new"), sibling("old"));
    let _ = std::fs::remove_dir_all(&new);
    let built = build_bundle(&exe, &new);
    if let Err(e) = built {
        let _ = std::fs::remove_dir_all(&new);
        return Err(e);
    }
    // Swap: move the old app aside, put the new one in its place, then remove the old one.
    let had_old = app.exists();
    if had_old {
        std::fs::rename(app, &old).map_err(|e| {
            let _ = std::fs::remove_dir_all(&new);
            format!("cannot move the old app aside: {e}")
        })?;
    }
    if let Err(e) = std::fs::rename(&new, app) {
        if had_old {
            let _ = std::fs::rename(&old, app);
        }
        let _ = std::fs::remove_dir_all(&new);
        return Err(format!("cannot put the new app in place: {e}"));
    }
    if had_old {
        let _ = std::fs::remove_dir_all(&old);
    }
    Ok(())
}

/// Write a complete, signed bundle at `app`, which must not exist yet.
fn build_bundle(exe: &std::path::Path, app: &std::path::Path) -> Result<(), String> {
    let macos = app.join("Contents/MacOS");
    std::fs::create_dir_all(&macos).map_err(|e| format!("cannot create {}: {e}", macos.display()))?;
    std::fs::copy(exe, macos.join(PROGRAM)).map_err(|e| format!("cannot copy the program: {e}"))?;
    std::fs::write(app.join("Contents/Info.plist"), info_plist()).map_err(|e| format!("cannot write Info.plist: {e}"))?;
    let resources = app.join("Contents/Resources");
    std::fs::create_dir_all(&resources).map_err(|e| format!("cannot create {}: {e}", resources.display()))?;
    std::fs::write(resources.join("AppIcon.icns"), APP_ICON).map_err(|e| format!("cannot write the icon: {e}"))?;
    // Ad-hoc signature: enough for macOS to remember the permission on this Mac.
    let sign = Command::new("/usr/bin/codesign")
        .args(["--force", "--deep", "--sign", "-", "--identifier", BUNDLE_ID, "--requirements"])
        // Tie the permission to the app id, not to this exact build, so it survives updates.
        .arg(format!("=designated => identifier \"{BUNDLE_ID}\""))
        .arg(app)
        .output()
        .map_err(|e| format!("cannot run codesign: {e}"))?;
    if !sign.status.success() {
        return Err(format!("codesign failed: {}", String::from_utf8_lossy(&sign.stderr).trim()));
    }
    Ok(())
}

pub fn open_accessibility_settings() {
    let _ = Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn();
}

/// Clear this app's stale Accessibility entry (it belonged to an older build), then ask again.
pub fn repair_permission() -> String {
    let out = Command::new("/usr/bin/tccutil").args(["reset", "Accessibility", BUNDLE_ID]).output();
    let ok = out.as_ref().map(|o| o.status.success()).unwrap_or(false);
    crate::ax::request_trust();
    open_accessibility_settings();
    if ok {
        "Old permission cleared. Turn on \"Keyclash\" in the list that just opened.".into()
    } else {
        "Could not clear the old entry. In the list, select Keyclash, press − to remove it, then press \"Ask macOS for permission\".".into()
    }
}
