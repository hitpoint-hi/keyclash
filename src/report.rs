//! The clash list as a Markdown document, to keep, print or share.

use crate::conflicts::{Conflict, Kind};
use crate::fix::{fix_for, free_keys_near, used_triggers};
use crate::keys::trigger_text;
use crate::model::*;

/// Rows shown per group before the rest is counted.
const MAX_ROWS: usize = 12;

fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

fn works(s: &Shortcut) -> &'static str {
    match s.scope {
        Scope::Global => "System-wide",
        Scope::App => "In app",
        Scope::Mode => "In a mode",
    }
}

pub fn markdown(list: &[Shortcut], conflicts: &[Conflict]) -> String {
    let used = used_triggers(list);
    let apps: std::collections::HashSet<String> = list.iter().map(|s| s.owner()).collect();
    let mut out = String::new();
    out.push_str("# Keyclash: keyboard shortcut clashes\n\n");
    out.push_str(&format!("{} shortcuts from {} apps and tools on this Mac.\n\n", list.iter().filter(|s| s.enabled).count(), apps.len()));
    out.push_str("| Kind | Count | Meaning |\n|---|---|---|\n");
    for k in [Kind::Clash, Kind::Blocks, Kind::SameModifier, Kind::InsideApp, Kind::Shared] {
        let n = conflicts.iter().filter(|c| c.kind == k).count();
        out.push_str(&format!("| {} | {n} | {} |\n", k.title(), k.explain()));
    }
    for k in [Kind::Clash, Kind::Blocks, Kind::SameModifier, Kind::InsideApp] {
        let group: Vec<&Conflict> = conflicts.iter().filter(|c| c.kind == k).collect();
        if group.is_empty() {
            continue;
        }
        out.push_str(&format!("\n## {} ({})\n\n{}\n", k.title(), group.len(), k.explain()));
        for c in group {
            out.push_str(&format!("\n### {}\n\n", c.label));
            out.push_str("| App | Action | Works | Found in | How to change |\n|---|---|---|---|---|\n");
            let mut members: Vec<&Shortcut> = c.members.iter().map(|&i| &list[i]).collect();
            members.sort_by_key(|s| (s.scope != Scope::Global, s.app.to_lowercase()));
            for s in members.iter().take(MAX_ROWS) {
                let keys = if k == Kind::SameModifier { format!("{}: ", trigger_text(&s.trigger)) } else { String::new() };
                let found = if s.estimated { format!("{} (guess)", s.origin.label()) } else { s.origin.label().to_string() };
                out.push_str(&format!(
                    "| {} | {keys}{} | {} | {} | {} |\n",
                    cell(&s.app),
                    cell(&s.action),
                    works(s),
                    found,
                    cell(&fix_for(s).how)
                ));
            }
            if members.len() > MAX_ROWS {
                out.push_str(&format!("\nAnd {} more apps.\n", members.len() - MAX_ROWS));
            }
            let free = free_keys_near(&c.trigger, &used, 5);
            if !free.is_empty() && matches!(k, Kind::Clash | Kind::Blocks) {
                let keys: Vec<String> = free.iter().map(trigger_text).collect();
                out.push_str(&format!("\nFree with the same modifier keys: {}\n", keys.join(", ")));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_a_clash_with_how_to_change_it() {
        let t = Trigger::combo(CTRL | OPT, "F");
        let list = vec![
            Shortcut::new("Rectangle", "Center Third", t.clone(), Scope::Global, Origin::BuiltInDefault).bundle(Some("com.knollsoft.Rectangle")),
            Shortcut::new("OmniWM", "Expand", t.clone(), Scope::Global, Origin::ConfigFile).bundle(Some("com.barut.OmniWM")).detail("/Users/me/.config/omniwm/settings.toml · expand = \"Control+Option+F\""),
        ];
        let md = markdown(&list, &crate::conflicts::find(&list));
        assert!(md.contains("## Clash (1)"));
        assert!(md.contains("### ⌃⌥F"));
        assert!(md.contains("| OmniWM | Expand | System-wide | Config file | Change the keys in /Users/me/.config/omniwm/settings.toml."));
        assert!(md.contains("Free with the same modifier keys: ⌃⌥"));
    }
}
