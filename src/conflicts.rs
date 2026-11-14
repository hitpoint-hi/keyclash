//! Group shortcuts by what you press and decide which groups are real clashes.

use crate::keys::trigger_text;
use crate::model::*;
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// Two or more system-wide shortcuts on the same keys: only one can win.
    Clash,
    /// A system-wide shortcut takes the keys before an app's own menu item sees them.
    Blocks,
    /// Tools that all use one modifier key by itself (tap, double-tap or hold).
    SameModifier,
    /// One app has two menu items on the same keys.
    InsideApp,
    /// Different apps use the same keys in their own menus. Normal: each works in its own app.
    Shared,
}

impl Kind {
    pub fn title(self) -> &'static str {
        match self {
            Kind::Clash => "Clash",
            Kind::Blocks => "Blocks an app",
            Kind::SameModifier => "Same key by itself",
            Kind::InsideApp => "Twice in one app",
            Kind::Shared => "Shared by apps",
        }
    }
    pub fn explain(self) -> &'static str {
        match self {
            Kind::Clash => "Two system-wide shortcuts use these keys. Only one of them will run.",
            Kind::Blocks => "An active system-wide shortcut can take these keys before another app receives them.",
            Kind::SameModifier => "Several tools listen to this modifier key by itself. Taps and holds can trigger each other.",
            Kind::InsideApp => "One app gives these keys to two menu items. Usually one is hidden or only shows in some windows.",
            Kind::Shared => "Each app uses these keys only while it is in front. This is normal.",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Conflict {
    pub kind: Kind,
    pub trigger: Trigger,
    pub label: String,
    /// Indexes into the shortcut list.
    pub members: Vec<usize>,
    pub owners: usize,
}

impl Conflict {
    pub fn is_active(&self, list: &[Shortcut], idle: &dyn Fn(&Shortcut) -> bool) -> bool {
        let open_owners: HashSet<String> = self.members.iter().map(|&i| &list[i])
            .filter(|s| s.scope == Scope::Global && !idle(s)).map(Shortcut::owner).collect();
        match self.kind {
            Kind::Clash | Kind::SameModifier => open_owners.len() >= 2,
            Kind::Blocks => !open_owners.is_empty(),
            Kind::InsideApp | Kind::Shared => true,
        }
    }
}

fn owner_set(list: &[Shortcut], idx: &[usize]) -> HashSet<String> {
    idx.iter().map(|&i| list[i].owner()).collect()
}

/// Shortcuts that count: turned on, and not only inside a special mode.
fn active(s: &Shortcut) -> bool {
    s.enabled && s.scope != Scope::Mode
}

pub fn find(list: &[Shortcut]) -> Vec<Conflict> {
    let mut by_trigger: BTreeMap<Trigger, Vec<usize>> = BTreeMap::new();
    for (i, s) in list.iter().enumerate() {
        if active(s) {
            by_trigger.entry(s.trigger.clone()).or_default().push(i);
        }
    }
    let mut out = Vec::new();
    for (t, idx) in &by_trigger {
        if idx.len() < 2 {
            continue;
        }
        let global: Vec<usize> = idx.iter().copied().filter(|&i| list[i].scope == Scope::Global).collect();
        let app: Vec<usize> = idx.iter().copied().filter(|&i| list[i].scope == Scope::App).collect();
        let g_owners = owner_set(list, &global);
        let all_owners = owner_set(list, idx);
        let kind = if g_owners.len() >= 2 {
            Kind::Clash
        } else if g_owners.len() == 1 && !app.is_empty() && all_owners.len() >= 2 {
            // macOS and the app's own menu item do the same thing (Minimize, Fill, Center…): no block.
            let same_action = || {
                let sys: HashSet<String> = global.iter().map(|&i| list[i].action.to_lowercase()).collect();
                app.iter().all(|&i| sys.contains(&list[i].action.to_lowercase()))
            };
            if g_owners.contains(crate::sources::MACOS) && (mirrored_by_menus(t) || same_action()) {
                Kind::Shared
            } else {
                Kind::Blocks
            }
        } else if all_owners.len() >= 2 {
            Kind::Shared
        } else {
            Kind::InsideApp
        };
        if kind == Kind::InsideApp {
            // Same title twice is one item read from two places, not a clash.
            let titles: HashSet<String> = idx.iter().map(|&i| list[i].action.to_lowercase()).collect();
            if titles.len() < 2 {
                continue;
            }
        }
        out.push(Conflict { kind, trigger: t.clone(), label: trigger_text(t), members: idx.clone(), owners: all_owners.len() });
    }

    // Modifier keys used by themselves (tap, double-tap, hold) across tools.
    let mut by_mod: BTreeMap<u8, Vec<usize>> = BTreeMap::new();
    for (i, s) in list.iter().enumerate() {
        if !active(s) {
            continue;
        }
        if let Some(m) = s.trigger.bare_modifier() {
            for bit in [CMD, SHIFT, OPT, CTRL, FN] {
                if m & bit != 0 {
                    by_mod.entry(bit).or_default().push(i);
                }
            }
        }
    }
    for (bit, idx) in by_mod {
        let owners = owner_set(list, &idx);
        let triggers: HashSet<&Trigger> = idx.iter().map(|&i| &list[i].trigger).collect();
        // Exact same trigger is already listed above.
        if owners.len() >= 2 && triggers.len() >= 2 {
            out.push(Conflict {
                kind: Kind::SameModifier,
                trigger: Trigger::ModTap { mods: bit, taps: 1 },
                label: format!("{} key by itself", crate::keys::mod_words(bit)),
                members: idx,
                owners: owners.len(),
            });
        }
    }
    out.sort_by(|a, b| a.kind.cmp(&b.kind).then(b.owners.cmp(&a.owners)).then(a.label.cmp(&b.label)));
    out
}

/// macOS shortcuts that every app also shows as its own menu item (Minimize, next window, Help).
/// macOS and the app do the same thing with them, so they do not block anything.
pub fn mirrored_by_menus(t: &Trigger) -> bool {
    let Trigger::Combo { mods, key } = t else { return false };
    let k = key.as_str();
    (*mods == CMD && (k == "M" || k == "`")) || (*mods == CMD | SHIFT && k == "/")
}

/// Shortcuts every Mac app shares; hidden from the "Shared by apps" list unless asked.
pub fn is_standard(t: &Trigger) -> bool {
    let Trigger::Combo { mods, key } = t else { return false };
    let k = key.as_str();
    match *mods {
        CMD => matches!(k, "Q" | "W" | "H" | "M" | "C" | "V" | "X" | "A" | "Z" | "N" | "O" | "S" | "P" | "F" | "G" | "E" | "J" | "," | "?" | "/" | "T" | "R" | "+" | "=" | "-" | "0" | "`" | "I" | "B" | "U" | "L" | "D" | "K" | "Y" | "[" | "]" | ";" | ":" | "." | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"),
        m if m == CMD | SHIFT => matches!(k, "Z" | "S" | "G" | "T" | "N" | "W" | "?" | "/" | "[" | "]" | "{" | "}" | "V" | "A" | "P" | "O" | "D" | "F" | "L" | "K" | "R" | "E" | "C" | "+" | "=" | "-" | ":" | ";"),
        m if m == CMD | OPT => matches!(k, "H" | "M" | "W" | "V" | "I" | "C" | "S" | "T" | "F" | "Escape" | "D" | "Z" | "N" | "P" | "L"),
        m if m == CMD | CTRL => matches!(k, "F" | "Space" | "D" | "Q"),
        m if m == CMD | OPT | SHIFT => matches!(k, "V" | "W" | "S"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sc(app: &str, action: &str, t: Trigger, scope: Scope) -> Shortcut {
        Shortcut::new(app, action, t, scope, Origin::ConfigFile).bundle(Some(app))
    }

    fn kind_of(list: &[Shortcut], t: &Trigger) -> Option<Kind> {
        find(list).into_iter().find(|c| &c.trigger == t).map(|c| c.kind)
    }

    #[test]
    fn kinds() {
        let t = Trigger::combo(CTRL | OPT, "F");
        let two_tools = [sc("Rectangle", "Center Third", t.clone(), Scope::Global), sc("OmniWM", "Expand", t.clone(), Scope::Global)];
        assert_eq!(kind_of(&two_tools, &t), Some(Kind::Clash));
        let tool_and_menu = [sc("OmniWM", "Toggle", t.clone(), Scope::Global), sc("Photos", "Grid", t.clone(), Scope::App)];
        assert_eq!(kind_of(&tool_and_menu, &t), Some(Kind::Blocks));
        let menus = [sc("Mail", "A", t.clone(), Scope::App), sc("Notes", "B", t.clone(), Scope::App)];
        assert_eq!(kind_of(&menus, &t), Some(Kind::Shared));
        // A shortcut that only works inside a tool's own mode or window does not count.
        let mode = [sc("Maccy", "Pin", t.clone(), Scope::Mode), sc("Notes", "B", t.clone(), Scope::App)];
        assert_eq!(kind_of(&mode, &t), None);
    }

    #[test]
    fn macos_minimize_does_not_block_app_menus() {
        let t = Trigger::combo(CMD, "M");
        let list = [sc(crate::sources::MACOS, "Minimize", t.clone(), Scope::Global), sc("Mail", "Minimize", t.clone(), Scope::App)];
        assert_eq!(kind_of(&list, &t), Some(Kind::Shared));
        // The Window menu of every app repeats the macOS tiling keys under the same name.
        let t = Trigger::combo(CTRL | FN, "C");
        let list = [sc(crate::sources::MACOS, "Center", t.clone(), Scope::Global), sc("Finder", "Center", t.clone(), Scope::App)];
        assert_eq!(kind_of(&list, &t), Some(Kind::Shared));
        // A macOS shortcut that apps do not mirror still blocks.
        let t = Trigger::combo(CMD | SHIFT, "4");
        let list = [sc(crate::sources::MACOS, "Screenshot", t.clone(), Scope::Global), sc("Photos", "Rate", t.clone(), Scope::App)];
        assert_eq!(kind_of(&list, &t), Some(Kind::Blocks));
    }

    #[test]
    fn double_tap_and_hold_of_one_modifier() {
        let list = [
            sc("Zethos", "Quick entry", Trigger::ModTap { mods: OPT, taps: 2 }, Scope::Global),
            sc("Other", "Talk", Trigger::ModHold { mods: OPT }, Scope::Global),
        ];
        assert!(find(&list).iter().any(|c| c.kind == Kind::SameModifier));
    }

    #[test]
    fn inactive_tools_do_not_make_a_current_clash_or_block() {
        let trigger = Trigger::combo(OPT, "K");
        let list = [
            sc("Tool A", "Action A", trigger.clone(), Scope::Global),
            sc("Tool B", "Action B", trigger.clone(), Scope::Global),
            sc("Editor", "Action", trigger, Scope::App),
        ];
        let clash = find(&list).remove(0);
        assert!(!clash.is_active(&list, &|s| s.app == "Tool A"));
        assert!(clash.is_active(&list, &|_| false));
        let block_list = &list[1..];
        let block = find(block_list).remove(0);
        assert!(!block.is_active(block_list, &|s| s.app == "Tool B"));
        assert!(block.is_active(block_list, &|_| false));
    }
}
