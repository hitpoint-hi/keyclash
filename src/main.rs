//! Keyclash: every keyboard shortcut on this Mac, across all apps, and where they clash.

mod agent;
mod ax;
mod bundle;
mod conflicts;
mod electron;
mod favorites;
mod fix;
mod hotkey;
mod icons;
mod keyboard;
mod keys;
mod layout;
mod learn;
mod model;
mod mcp;
mod nib;
mod panel;
mod report;
mod remap;
mod scan;
mod settings;
mod sources;
mod theme;
mod ui;
mod usage;
mod use_tab;
mod welcome;
mod window;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let screenshot = args
        .iter()
        .position(|a| a == "--screenshot")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);

    // The installed app takes the files of earlier versions into its data folder.
    if bundle::running_as_app() {
        sources::move_old_data();
    }

    // `--bundle <folder>`: make Keyclash.app in that folder from this program (for a download).
    if let Some(dir) = args.iter().position(|a| a == "--bundle").and_then(|i| args.get(i + 1)) {
        let app = PathBuf::from(dir).join(format!("{}.app", bundle::APP_NAME));
        match bundle::make_bundle(&app) {
            Ok(()) => println!("Made {}", app.display()),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    // `--install`: make ~/Applications/Keyclash.app from this program and open it.
    if args.iter().any(|a| a == "--install") {
        match bundle::install_and_open() {
            Ok(p) => println!("Installed and opened {}", p.display()),
            Err(e) => {
                eprintln!("Install failed: {e}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    // `--layout-dump [name]`: print the key on each character key of the selected layout, or of an installed one.
    if let Some(i) = args.iter().position(|a| a == "--layout-dump") {
        let found = match args.get(i + 1) {
            Some(name) => layout::read_named(name),
            None => layout::read_current(),
        };
        match found {
            Some(l) => {
                println!("{} ({})", l.name, l.id);
                for code in layout::CHARACTER_KEYS {
                    let us = keys::keycode_name(code).unwrap_or("?");
                    let now = l.keys.get(&code).map(String::as_str).unwrap_or("(none)");
                    println!("{code:>3}  US {us:<3} -> {now}{}", if us == now { "" } else { "   *" });
                }
            }
            None => eprintln!("no such keyboard layout"),
        }
        return Ok(());
    }

    // Key names follow the keyboard layout. `--layout <name>` uses an installed layout for this run.
    let forced_layout = args.iter().position(|a| a == "--layout").and_then(|i| args.get(i + 1));
    layout::set(match forced_layout {
        Some(name) => layout::read_named(name),
        None => layout::read_current(),
    });

    let deep = args.iter().any(|a| a == "--deep");
    if args.iter().any(|a| a == "--mcp") {
        mcp::serve(deep);
        return Ok(());
    }

    if let Some(i) = args.iter().position(|a| a == "--tool") {
        let result = (|| {
            let name = args.get(i + 1).ok_or("Use --tool <name> '[JSON arguments]'.")?;
            if name == "help" {
                return Ok(agent::tool_specs());
            }
            if !agent::TOOL_NAMES.contains(&name.as_str()) {
                return Err(format!("Unknown tool: {name}. Use --tool help."));
            }
            let input = args.get(i + 2).filter(|s| !s.starts_with("--")).map(String::as_str).unwrap_or("{}");
            let input: serde_json::Value = serde_json::from_str(input).map_err(|e| format!("Invalid JSON arguments: {e}"))?;
            agent::Session::new(deep).call(name, &input)
        })();
        match result {
            Ok(value) => println!("{}", serde_json::to_string_pretty(&value).expect("JSON value")),
            Err(message) => {
                eprintln!("{}", serde_json::json!({ "error": message }));
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    if args.iter().any(|a| a == "--report") {
        if args.iter().any(|a| a == "--json") {
            let result = agent::run_scan(deep);
            println!("{}", serde_json::to_string_pretty(&agent::report_json(&result, deep, true)).expect("JSON report"));
        } else {
            print_report();
        }
        return Ok(());
    }

    // `--export <file.md>`: write the clash list as Markdown (no window). Add `--deep` for live menus.
    if let Some(path) = args.iter().position(|a| a == "--export").and_then(|i| args.get(i + 1)) {
        let q = scan::quick_scan();
        let d = args.iter().any(|a| a == "--deep").then(|| scan::deep_scan(&q));
        let r = scan::merge(&q, d.as_ref());
        let md = report::markdown(&r.shortcuts, &conflicts::find(&r.shortcuts));
        match std::fs::write(path, md) {
            Ok(()) => println!("Wrote {path}"),
            Err(e) => eprintln!("cannot write {path}: {e}"),
        }
        return Ok(());
    }

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1240.0, 820.0])
        .with_min_inner_size([900.0, 560.0])
        .with_title("Keyclash");
    if let Some(icon) = bundle::window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "Keyclash",
        options,
        Box::new(move |cc| Ok(Box::new(ui::AtlasApp::new(cc, screenshot)))),
    )
}

/// `--report`: print the clashes as text (no window). Useful for a quick check.
fn print_report() {
    let deep = std::env::args().any(|a| a == "--deep");
    let q = scan::quick_scan();
    let d = deep.then(|| scan::deep_scan(&q));
    let r = scan::merge(&q, d.as_ref());
    for n in &r.notes {
        println!("[source] {}: {} ({})", n.name, n.count, n.status);
    }
    if std::env::args().any(|a| a == "--dump") {
        for s in &r.shortcuts {
            println!("{}\t{}\t{}\t{:?}\t{:?}\t{}", keys::trigger_text(&s.trigger), s.app, s.action, s.scope, s.origin, s.detail);
        }
        return;
    }
    let c = conflicts::find(&r.shortcuts);
    for x in c.iter().filter(|x| x.kind != conflicts::Kind::Shared) {
        println!("\n{} — {}", x.label, x.kind.title());
        for &i in &x.members {
            let s = &r.shortcuts[i];
            println!("    {:<22} {:<40} {:?} {:?}", s.app, s.action, s.scope, s.origin);
        }
    }
    println!("\n{} shortcuts, {} groups", r.shortcuts.len(), c.len());
}
