//! Keyclash for AI agents: the scan as JSON, and the tools that `--tool` and the MCP server share.
//!
//! Every tool returns JSON. Read tools never change anything. `propose_change` only makes a plan;
//! `apply_change` writes it (with a backup) and `undo_last` puts the last change back.

use crate::conflicts::{self, Conflict, Kind};
use crate::fix::{fix_for, free_keys_near, used_triggers};
use crate::keys::{LookupQuery, check_lookup, check_typed, mod_symbols, modifier_text, modifier_word, trigger_text, typed_text};
use crate::model::*;
use crate::remap;
use crate::scan::{self, ScanResult};
use serde_json::{Value, json};
use std::collections::HashSet;

pub fn run_scan(deep: bool) -> ScanResult {
    let q = scan::quick_scan();
    let d = deep.then(|| scan::deep_scan(&q));
    scan::merge(&q, d.as_ref())
}

/// A stable id for one shortcut. A menu item keeps its id when it is read from a file, from memory or live.
pub fn shortcut_id(s: &Shortcut) -> String {
    let source = match s.origin {
        Origin::LiveMenu | Origin::RememberedMenu | Origin::MenuFile => "menu".to_string(),
        o => format!("{o:?}|{}", s.detail),
    };
    remap::short_id("k", &format!("{}|{}|{}|{:?}|{source}", s.owner(), s.action, crate::usage::code(&s.trigger), s.scope))
}

fn works(s: &Shortcut) -> &'static str {
    match s.scope {
        Scope::Global => "system-wide",
        Scope::App => "in-app",
        Scope::Mode => "in-mode",
    }
}

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Clash => "clash",
        Kind::Blocks => "blocks_an_app",
        Kind::SameModifier => "same_modifier_key",
        Kind::InsideApp => "twice_in_one_app",
        Kind::Shared => "shared_by_apps",
    }
}

fn keys_json(t: &Trigger) -> Value {
    json!({ "keys": trigger_text(t), "typed": typed_text(t) })
}

pub fn shortcut_json(s: &Shortcut, presses: Option<u32>) -> Value {
    let mut v = json!({
        "id": shortcut_id(s),
        "keys": trigger_text(&s.trigger),
        "typed": typed_text(&s.trigger),
        "app": s.app,
        "bundle": s.bundle,
        "action": s.action,
        "works": works(s),
        "found_in": s.origin.label(),
        "detail": s.detail,
    });
    let o = v.as_object_mut().expect("object");
    if !s.enabled {
        o.insert("enabled".into(), json!(false));
    }
    if s.estimated {
        o.insert("guess".into(), json!(true));
    }
    match remap::writer_for(s) {
        Ok(w) => {
            o.insert("keyclash_can_change".into(), json!(w.label()));
        }
        Err(_) => {
            o.insert("how_to_change".into(), json!(fix_for(s).how));
        }
    }
    if let Some(n) = presses {
        o.insert("presses_30_days".into(), json!(n));
    }
    v
}

/// Tools whose system-wide hotkeys are idle now: the app is not open (same rule as the window).
fn not_running(r: &ScanResult) -> HashSet<String> {
    r.apps.iter().filter(|a| !a.running).filter_map(|a| a.bundle.clone()).collect()
}

fn idle(nr: &HashSet<String>, s: &Shortcut) -> bool {
    s.scope == Scope::Global && s.origin != Origin::KeyRemap && s.bundle.as_ref().is_some_and(|b| nr.contains(b))
}

fn conflict_json(c: &Conflict, list: &[Shortcut], used: &HashSet<Trigger>, nr: &HashSet<String>) -> Value {
    let mut members: Vec<&Shortcut> = c.members.iter().map(|&i| &list[i]).collect();
    members.sort_by_key(|s| (s.scope != Scope::Global, s.app.to_lowercase()));
    let free: Vec<Value> = if matches!(c.kind, Kind::Clash | Kind::Blocks) { free_keys_near(&c.trigger, used, 5).iter().map(keys_json).collect() } else { vec![] };
    json!({
        "kind": kind_name(c.kind),
        "keys": c.label,
        "typed": typed_text(&c.trigger),
        "meaning": c.kind.explain(),
        "active_now": c.is_active(list, &|s| idle(nr, s)),
        "members": members.iter().map(|s| shortcut_json(s, None)).collect::<Vec<_>>(),
        "free_nearby": free,
    })
}

fn kind_counts(c: &[Conflict]) -> Value {
    let mut m = serde_json::Map::new();
    for k in [Kind::Clash, Kind::Blocks, Kind::SameModifier, Kind::InsideApp, Kind::Shared] {
        m.insert(kind_name(k).into(), json!(c.iter().filter(|x| x.kind == k).count()));
    }
    Value::Object(m)
}

pub fn summary_json(r: &ScanResult, deep: bool) -> Value {
    let c = conflicts::find(&r.shortcuts);
    let apps: HashSet<String> = r.shortcuts.iter().map(|s| s.owner()).collect();
    json!({
        "shortcuts": r.shortcuts.iter().filter(|s| s.enabled).count(),
        "apps_and_tools": apps.len(),
        "conflicts": kind_counts(&c),
        "keyboard_layout": crate::layout::active_name().unwrap_or_else(|| "U.S.".into()),
        "deep_scan_requested": deep,
        "live_menus_read": !r.live_bundles.is_empty(),
        "live_menu_apps": r.live_bundles.len(),
        "accessibility_permission": r.ax_trusted,
        "use_counter_on": !crate::usage::paused(),
        "applied_changes": remap::applied_log().ok().map(|log| log.iter().filter(|a| !a.pending).count()),
        "sources": r.notes.iter().map(|n| json!({ "name": n.name, "count": n.count, "status": n.status })).collect::<Vec<_>>(),
    })
}

/// The whole report for `--report --json`: summary and conflicts, and every shortcut with `all`.
pub fn report_json(r: &ScanResult, deep: bool, all: bool) -> Value {
    let mut v = summary_json(r, deep);
    let c = conflicts::find(&r.shortcuts);
    let used = used_triggers(&r.shortcuts);
    let nr = not_running(r);
    let list: Vec<Value> = c.iter().filter(|x| x.kind != Kind::Shared).map(|x| conflict_json(x, &r.shortcuts, &used, &nr)).collect();
    let o = v.as_object_mut().expect("object");
    o.insert("conflict_list".into(), json!(list));
    let mut modifiers = vec![CMD, CMD | SHIFT, CTRL | OPT, CTRL | OPT | CMD, CTRL | OPT | CMD | SHIFT];
    modifiers.extend(r.shortcuts.iter().map(|s| s.trigger.mods()).filter(|m| m & !SHIFT != 0));
    modifiers.sort_unstable();
    modifiers.dedup();
    o.insert("free_keys".into(), json!(modifiers.into_iter().map(|m| json!({
        "modifiers": crate::keys::mod_symbols(m).concat(),
        "free": free_with(m, &used, None).iter().map(keys_json).collect::<Vec<_>>()
    })).collect::<Vec<_>>()));
    o.insert("schema_version".into(), json!(1));
    o.insert("limits".into(), json!("Free means no enabled shortcut outside special modes in this scan uses these keys. Unread sources and runtime behavior can differ. Config writes need the reported reload or restart before the app uses them."));
    if all {
        o.insert("all_shortcuts".into(), json!(r.shortcuts.iter().map(|s| shortcut_json(s, None)).collect::<Vec<_>>()));
    }
    v
}

/// Modifier words or symbols ("ctrl opt", "⌃⌥", "hyper") to modifier bits.
pub fn parse_mods(text: &str) -> Result<u8, String> {
    let mut m = 0;
    let mut rest = String::new();
    for c in text.chars() {
        let bit = match c {
            '⌘' => CMD,
            '⇧' => SHIFT,
            '⌥' => OPT,
            '⌃' => CTRL,
            _ => 0,
        };
        m |= bit;
        rest.push(if bit == 0 { c } else { ' ' });
    }
    for w in rest.split(|c: char| c.is_whitespace() || c == '+' || c == '-' || c == ',').filter(|w| !w.is_empty()) {
        m |= modifier_word(w).ok_or_else(|| format!("\"{w}\" is not a modifier key. Use cmd, shift, opt, ctrl, or fn."))?;
    }
    if m == 0 {
        return Err("Name one or more modifier keys, for example \"ctrl opt\".".into());
    }
    Ok(m)
}

/// Every free combination with these modifier keys on the letter, digit and punctuation keys.
pub fn free_with(mods: u8, used: &HashSet<Trigger>, near: Option<&str>) -> Vec<Trigger> {
    let from = Trigger::combo(mods, near.unwrap_or("G").to_uppercase());
    let mut out = free_keys_near(&from, used, 60);
    let key = near.unwrap_or("G").to_uppercase();
    if mods & !SHIFT != 0 && "`1234567890-=QWERTYUIOP[]\\ASDFGHJKL;'ZXCVBNM,./".contains(&key) && key.chars().count() == 1 && !used.contains(&from) {
        out.insert(0, from);
    }
    out
}

fn press_counts(list: &[Shortcut]) -> Option<Vec<u32>> {
    let (_, rows) = crate::usage::snapshot(crate::usage::Range::Month);
    (!rows.is_empty()).then(|| crate::usage::attribute(list, &rows).per_shortcut)
}

fn arg_str<'a>(args: &'a Value, k: &str) -> Option<&'a str> {
    args.get(k).and_then(Value::as_str).filter(|s| !s.trim().is_empty())
}

fn need_str<'a>(args: &'a Value, k: &str) -> Result<&'a str, String> {
    arg_str(args, k).ok_or_else(|| format!("Missing \"{k}\"."))
}

fn arg_usize(args: &Value, k: &str, default: usize) -> usize {
    args.get(k).and_then(Value::as_u64).map(|n| n as usize).unwrap_or(default)
}

fn arg_bool(args: &Value, k: &str) -> bool {
    args.get(k).and_then(Value::as_bool).unwrap_or(false)
}

/// One agent session: a scan kept in memory and refreshed after each write.
pub struct Session {
    pub scan: ScanResult,
    pub deep: bool,
}

impl Session {
    pub fn new(deep: bool) -> Self {
        Session { scan: run_scan(deep), deep }
    }

    fn rescan(&mut self) {
        self.scan = run_scan(self.deep);
    }

    fn find(&self, id: &str) -> Result<&Shortcut, String> {
        self.scan
            .shortcuts
            .iter()
            .find(|s| shortcut_id(s) == id)
            .ok_or_else(|| format!("No shortcut has the id {id}. Ids change when keys change; list or look up the shortcut again."))
    }

    /// Conflicts that involve one shortcut: (kind, keys, other members).
    fn conflicts_of(list: &[Shortcut], idx: usize) -> Vec<Value> {
        conflicts::find(list)
            .iter()
            .filter(|c| c.members.contains(&idx))
            .map(|c| {
                let others: Vec<String> = c.members.iter().filter(|&&i| i != idx).map(|&i| format!("{}: {}", list[i].app, list[i].action)).collect();
                json!({ "kind": kind_name(c.kind), "keys": c.label, "with": others })
            })
            .collect()
    }

    pub fn call(&mut self, tool: &str, args: &Value) -> Result<Value, String> {
        validate_args(tool, args)?;
        let list = &self.scan.shortcuts;
        match tool {
            "summary" => Ok(summary_json(&self.scan, self.deep)),
            "conflicts" => {
                let used = used_triggers(list);
                let nr = not_running(&self.scan);
                let shared = arg_bool(args, "include_shared");
                let active_only = arg_bool(args, "active_only");
                let limit = arg_usize(args, "limit", 40);
                let all: Vec<Conflict> = conflicts::find(list)
                    .into_iter()
                    .filter(|c| shared || c.kind != Kind::Shared)
                    .filter(|c| !active_only || c.is_active(list, &|s| idle(&nr, s)))
                    .collect();
                let shown: Vec<Value> = all.iter().take(limit).map(|c| conflict_json(c, list, &used, &nr)).collect();
                Ok(json!({ "total": all.len(), "shown": shown.len(), "conflicts": shown }))
            }
            "list_shortcuts" => {
                let app = arg_str(args, "app").map(str::to_lowercase);
                let text = arg_str(args, "text").map(str::to_lowercase);
                let keys = arg_str(args, "keys").map(check_lookup).transpose()?;
                let scope = arg_str(args, "works");
                let limit = arg_usize(args, "limit", 100);
                let offset = arg_usize(args, "offset", 0);
                let presses = press_counts(list);
                let hits: Vec<usize> = (0..list.len())
                    .filter(|&i| {
                        let s = &list[i];
                        app.as_ref().is_none_or(|a| s.app.to_lowercase().contains(a) || s.bundle.as_deref().is_some_and(|b| b.to_lowercase() == *a))
                            && text.as_ref().is_none_or(|t| s.action.to_lowercase().contains(t))
                            && keys.as_ref().is_none_or(|query| query.matches(&s.trigger))
                            && scope.is_none_or(|w| works(s) == w)
                    })
                    .collect();
                let rows: Vec<Value> = hits.iter().skip(offset).take(limit).map(|&i| shortcut_json(&list[i], presses.as_ref().map(|p| p[i]))).collect();
                Ok(json!({ "total": hits.len(), "offset": offset, "shortcuts": rows }))
            }
            "lookup" => {
                let query = check_lookup(need_str(args, "keys")?)?;
                let used = used_triggers(list);
                let users: Vec<Value> = list.iter().filter(|s| query.matches(&s.trigger)).map(|s| shortcut_json(s, None)).collect();
                let also: Vec<Value> = match &query {
                    LookupQuery::Shortcut(Trigger::Combo { key, mods }) => {
                        let mut seen = HashSet::new();
                        list.iter()
                            .filter(|s| matches!(&s.trigger, Trigger::Combo { key: k, mods: m } if k == key && m != mods))
                            .filter(|s| seen.insert(s.trigger.clone()))
                            .map(|s| keys_json(&s.trigger))
                            .collect()
                    }
                    _ => vec![],
                };
                let (keys, typed, free_with_same_modifiers) = match &query {
                    LookupQuery::Shortcut(trigger) => (
                        trigger_text(trigger), typed_text(trigger),
                        free_keys_near(trigger, &used, 8).iter().map(keys_json).collect::<Vec<_>>(),
                    ),
                    LookupQuery::Modifiers(mods) => (mod_symbols(*mods).concat(), modifier_text(*mods), Vec::new()),
                };
                Ok(json!({
                    "keys": keys,
                    "typed": typed,
                    "free": !used.iter().any(|trigger| query.matches(trigger)),
                    "used_by": users,
                    "same_key_other_modifiers": also,
                    "free_with_same_modifiers": free_with_same_modifiers,
                }))
            }
            "free_keys" => {
                let mods = parse_mods(need_str(args, "modifiers")?)?;
                let used = used_triggers(list);
                let free = free_with(mods, &used, arg_str(args, "near"));
                Ok(json!({ "modifiers": crate::keys::mod_symbols(mods).concat(), "free": free.iter().map(keys_json).collect::<Vec<_>>() }))
            }
            "usage" => {
                let limit = arg_usize(args, "limit", 25);
                let (since, rows) = crate::usage::snapshot(crate::usage::Range::Month);
                if rows.is_empty() {
                    return Ok(json!({ "counter_on": !crate::usage::paused(), "note": "No presses are counted. Turn on Use › Count key presses in the Keyclash window." }));
                }
                let rep = crate::usage::attribute(list, &rows);
                let mut idx: Vec<usize> = (0..list.len()).filter(|&i| rep.per_shortcut[i] > 0).collect();
                idx.sort_by(|a, b| rep.per_shortcut[*b].cmp(&rep.per_shortcut[*a]));
                let top: Vec<Value> = idx.iter().take(limit).map(|&i| shortcut_json(&list[i], Some(rep.per_shortcut[i]))).collect();
                let starred_unused: Vec<Value> = (0..list.len())
                    .filter(|&i| rep.per_shortcut[i] == 0 && crate::favorites::is_starred(&list[i]))
                    .take(limit)
                    .map(|i| shortcut_json(&list[i], Some(0)))
                    .collect();
                let unknown: Vec<Value> = rep.unknown.iter().take(limit).map(|(t, b, n)| json!({ "keys": trigger_text(t), "app": b, "presses": n })).collect();
                Ok(json!({
                    "counting_since": since,
                    "days": 30,
                    "total_presses": rep.total,
                    "most_used": top,
                    "starred_not_pressed": starred_unused,
                    "pressed_but_not_in_scan": unknown,
                    "note": "A key press does not prove that the action ran. Presses with several possible actions are not credited to any of them.",
                }))
            }
            "propose_change" => {
                let id = need_str(args, "id")?;
                let new = check_typed(need_str(args, "new_keys")?)?;
                let idx = list.iter().position(|s| shortcut_id(s) == id).ok_or_else(|| self.find(id).err().unwrap_or_default())?;
                let s = &list[idx];
                let plan = remap::propose(s, id, &new)?;
                remap::save_plan(&plan)?;
                let before = Self::conflicts_of(list, idx);
                let mut sim = list.clone();
                sim[idx].trigger = new.clone();
                let after = Self::conflicts_of(&sim, idx);
                let other_users: Vec<String> = list.iter().filter(|x| x.trigger == new).map(|x| format!("{} ({}): {}", x.app, works(x), x.action)).collect();
                Ok(json!({
                    "plan_id": plan.plan_id,
                    "shortcut": format!("{}: {}", plan.app, plan.action),
                    "old_keys": plan.old_keys,
                    "new_keys": plan.new_keys,
                    "writes_to": plan.writer,
                    "diff": plan.diff(),
                    "conflicts_now": before,
                    "conflicts_after": after,
                    "new_keys_also_used_by": other_users,
                    "after_apply": plan.after_apply,
                    "next": "Show the diff and the conflicts to the person. Call apply_change with this plan_id only after they agree.",
                }))
            }
            "apply_change" => {
                let pid = need_str(args, "plan_id")?;
                let plan = remap::find_plan(pid)?.ok_or_else(|| format!("No plan has the id {pid}. Call propose_change again."))?;
                let before = conflicts::find(list).iter().filter(|c| matches!(c.kind, Kind::Clash | Kind::Blocks)).count();
                let done = remap::apply(&plan)?;
                self.rescan();
                let after = conflicts::find(&self.scan.shortcuts).iter().filter(|c| matches!(c.kind, Kind::Clash | Kind::Blocks)).count();
                let new_id = self.scan.shortcuts.iter().find(|s| s.app == plan.app && s.action == plan.action && trigger_text(&s.trigger) == plan.new_keys).map(shortcut_id);
                Ok(json!({
                    "applied": plan.plan_id,
                    "shortcut": format!("{}: {}", plan.app, plan.action),
                    "keys": format!("{} → {}", plan.old_keys, plan.new_keys),
                    "backup": done.backup,
                    "clashes_and_blocks_before": before,
                    "clashes_and_blocks_after": after,
                    "new_id": new_id,
                    "after_apply": plan.after_apply,
                    "undo": "Call undo_last to put it back.",
                }))
            }
            "undo_last" => {
                let (done, restored) = remap::undo_last()?;
                if !restored {
                    return Ok(json!({
                        "cleared": done.plan.plan_id,
                        "shortcut": format!("{}: {}", done.plan.app, done.plan.action),
                        "note": "This change did not finish, and the target does not hold it. Keyclash restored nothing and removed the recovery record.",
                        "backup": done.backup,
                    }));
                }
                self.rescan();
                Ok(json!({
                    "undone": done.plan.plan_id,
                    "shortcut": format!("{}: {}", done.plan.app, done.plan.action),
                    "keys": format!("{} → {} (back to {})", done.plan.new_keys, done.plan.old_keys, done.plan.old_keys),
                    "after_apply": done.plan.after_apply,
                }))
            }
            "rescan" => {
                if let Some(d) = args.get("deep").and_then(Value::as_bool) {
                    self.deep = d;
                }
                self.rescan();
                Ok(summary_json(&self.scan, self.deep))
            }
            other => Err(format!("Unknown tool \"{other}\".")),
        }
    }
}

pub const TOOL_NAMES: [&str; 10] = ["summary", "conflicts", "list_shortcuts", "lookup", "free_keys", "usage", "propose_change", "apply_change", "undo_last", "rescan"];

fn validate_args(tool: &str, args: &Value) -> Result<(), String> {
    let specs = tool_specs();
    let spec = specs.as_array().expect("tool specs").iter().find(|s| s["name"] == tool).ok_or_else(|| format!("Unknown tool: {tool}"))?;
    let schema = &spec["inputSchema"];
    let object = args.as_object().ok_or("Tool arguments must be a JSON object.")?;
    if let Some(required) = schema["required"].as_array() {
        for name in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(name) { return Err(format!("Missing argument: {name}")); }
        }
    }
    for (name, value) in object {
        let property = schema["properties"].get(name).ok_or_else(|| format!("Unknown argument: {name}"))?;
        let valid = match property["type"].as_str() {
            Some("string") => value.is_string(),
            Some("boolean") => value.is_boolean(),
            Some("integer") => value.as_u64().is_some_and(|n| n >= property["minimum"].as_u64().unwrap_or(0)),
            _ => false,
        };
        if !valid || property["enum"].as_array().is_some_and(|values| !values.contains(value)) {
            return Err(format!("Invalid value for argument: {name}"));
        }
    }
    Ok(())
}

/// The tool list for MCP `tools/list` (and `--tool help`).
pub fn tool_specs() -> Value {
    let keys_help = "Keys as words or symbols, for example \"cmd shift t\", \"ctrl opt left\", \"⌃⌥⌘F\", \"double option\".";
    let read = json!({ "readOnlyHint": true, "openWorldHint": false });
    json!([
        {
            "name": "summary",
            "title": "Shortcut summary",
            "description": "Counts of shortcuts, apps and conflicts on this Mac, the keyboard layout, and which sources Keyclash read. Start here.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": read,
        },
        {
            "name": "conflicts",
            "title": "Shortcut conflicts",
            "description": "Shortcut conflicts. kind \"clash\": two system-wide hotkeys on the same keys (only one runs). \"blocks_an_app\": a system-wide hotkey takes keys that an app's menu item uses. \"same_modifier_key\": tools that all listen to one modifier key by itself. \"twice_in_one_app\": one app uses the keys twice. active_now is false when an owner app is not open. Each member has an id for propose_change, and free_nearby lists free keys with the same modifier keys.",
            "inputSchema": { "type": "object", "properties": {
                "include_shared": { "type": "boolean", "description": "Also list keys that different apps use in their own menus. This is normal and not a problem. Default false." },
                "active_only": { "type": "boolean", "description": "Only conflicts where the apps are open now. Default false." },
                "limit": { "type": "integer", "minimum": 1, "description": "Default 40." }
            } },
            "annotations": read,
        },
        {
            "name": "list_shortcuts",
            "title": "List shortcuts",
            "description": "Find shortcuts by app, action text, keys or scope. Each row has an id, the keys, the app, the action, where Keyclash found it, and either keyclash_can_change (Keyclash can write it) or how_to_change (manual steps).",
            "inputSchema": { "type": "object", "properties": {
                "app": { "type": "string", "description": "Part of the app name, or the exact bundle id." },
                "text": { "type": "string", "description": "Part of the action name." },
                "keys": { "type": "string", "description": keys_help },
                "works": { "type": "string", "enum": ["system-wide", "in-app", "in-mode"] },
                "limit": { "type": "integer", "minimum": 1, "description": "Default 100." },
                "offset": { "type": "integer", "minimum": 0 }
            } },
            "annotations": read,
        },
        {
            "name": "lookup",
            "title": "Look up keys",
            "description": "Who uses one key combination, the same key with other modifier keys, and free keys with the same modifier keys.",
            "inputSchema": { "type": "object", "properties": { "keys": { "type": "string", "description": keys_help } }, "required": ["keys"] },
            "annotations": read,
        },
        {
            "name": "free_keys",
            "title": "Free keys",
            "description": "Letter, digit and punctuation keys that no app or tool uses with these modifier keys, nearest to `near` first.",
            "inputSchema": { "type": "object", "properties": {
                "modifiers": { "type": "string", "description": "For example \"ctrl opt\", \"cmd shift\", \"⌃⌥⌘\"." },
                "near": { "type": "string", "description": "A key to start from, for example \"f\"." }
            }, "required": ["modifiers"] },
            "annotations": read,
        },
        {
            "name": "usage",
            "title": "Shortcut use",
            "description": "Shortcut presses in the last 30 days, from Keyclash's opt-in use counter: most used, starred but not pressed, and keys pressed that the scan does not know. Counts only; no typed text.",
            "inputSchema": { "type": "object", "properties": { "limit": { "type": "integer", "minimum": 1, "description": "Default 25." } } },
            "annotations": read,
        },
        {
            "name": "propose_change",
            "title": "Propose a new key",
            "description": "Plan new keys for one shortcut. Stores a local proposal without changing shortcut configuration. Returns a plan_id, the exact diff, the conflicts now and after, and other users of the new keys. Keyclash can change AeroSpace, OmniWM and skhd bindings, and app menu items (macOS App Shortcuts). For other shortcuts it returns the manual steps as an error.",
            "inputSchema": { "type": "object", "properties": {
                "id": { "type": "string", "description": "The shortcut id from conflicts, list_shortcuts or lookup." },
                "new_keys": { "type": "string", "description": keys_help }
            }, "required": ["id", "new_keys"] },
            "annotations": { "readOnlyHint": true, "openWorldHint": false },
        },
        {
            "name": "apply_change",
            "title": "Apply a planned change",
            "description": "Write a plan from propose_change. Ask the person first. Keeps a backup, refuses when the file changed after the plan, scans again, and returns the clash count before and after.",
            "inputSchema": { "type": "object", "properties": { "plan_id": { "type": "string" } }, "required": ["plan_id"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
        },
        {
            "name": "undo_last",
            "title": "Undo the last change",
            "description": "Put back the last change that apply_change wrote. Refuses when the file changed after Keyclash wrote it. For an apply that did not finish and left the target unchanged or edited by hand, it restores nothing, removes the recovery record, and answers with \"cleared\".",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
        },
        {
            "name": "rescan",
            "title": "Scan again",
            "description": "Read every source again. deep: true also reads app code, sandboxed settings and the live menus of open apps (slower; live menus need Accessibility permission for the program that runs Keyclash).",
            "inputSchema": { "type": "object", "properties": { "deep": { "type": "boolean" } } },
            "annotations": read,
        }
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(list: Vec<Shortcut>) -> Session {
        Session { scan: ScanResult { shortcuts: list, ..Default::default() }, deep: false }
    }

    fn sample() -> Vec<Shortcut> {
        let t = Trigger::combo(CTRL | OPT, "F");
        vec![
            Shortcut::new("Rectangle", "Center Third", t.clone(), Scope::Global, Origin::BuiltInDefault).bundle(Some("com.knollsoft.Rectangle")),
            Shortcut::new("OmniWM", "Expand", t, Scope::Global, Origin::ConfigFile).bundle(Some(remap::OMNIWM)).detail("/Users/me/.config/omniwm/settings.toml · expand = \"Control+Option+F\""),
            Shortcut::new("Mail", "Archive", Trigger::combo(CMD | CTRL, "A"), Scope::App, Origin::MenuFile).bundle(Some("com.apple.mail")),
        ]
    }

    #[test]
    fn ids_are_stable_and_follow_the_source() {
        let a = &sample()[2];
        let mut b = a.clone();
        b.origin = Origin::LiveMenu;
        b.detail = "Message › Archive".into();
        assert_eq!(shortcut_id(a), shortcut_id(&b));
        let ids: HashSet<String> = sample().iter().map(shortcut_id).collect();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn conflicts_tool_lists_the_clash_with_ids_and_free_keys() {
        let mut s = session(sample());
        let v = s.call("conflicts", &json!({})).unwrap();
        assert_eq!(v["total"], 1);
        let c = &v["conflicts"][0];
        assert_eq!(c["kind"], "clash");
        assert_eq!(c["typed"], "ctrl opt f");
        assert_eq!(c["members"].as_array().unwrap().len(), 2);
        assert!(!c["free_nearby"].as_array().unwrap().is_empty());
        let omni = c["members"].as_array().unwrap().iter().find(|m| m["app"] == "OmniWM").unwrap();
        assert_eq!(omni["keyclash_can_change"], "OmniWM config file");
        let rect = c["members"].as_array().unwrap().iter().find(|m| m["app"] == "Rectangle").unwrap();
        assert!(rect["how_to_change"].as_str().unwrap().contains("Rectangle"));
    }

    #[test]
    fn modifier_lookup_and_list_use_the_same_exact_modifier_set() {
        let mut s = session(vec![
            Shortcut::new("ChatGPT", "Hold to dictate", Trigger::ModHold { mods: CTRL }, Scope::Global, Origin::AppSettings),
            Shortcut::new("ChatGPT", "Toggle dictation", Trigger::ModTap { mods: CTRL, taps: 1 }, Scope::Global, Origin::AppSettings),
            Shortcut::new("Wispr Flow", "Command mode", Trigger::ModHold { mods: CTRL | FN }, Scope::Global, Origin::AppSettings),
        ]);
        let args = json!({ "keys": "ctrl" });
        let result = s.call("lookup", &args).unwrap();
        assert_eq!(result["typed"], "ctrl");
        assert_eq!(result["free"], false);
        assert_eq!(result["used_by"].as_array().unwrap().len(), 2);
        assert!(result["used_by"].as_array().unwrap().iter().all(|row| row["app"] == "ChatGPT"));
        let list = s.call("list_shortcuts", &args).unwrap();
        assert_eq!(list["total"], 2);
        assert!(list["shortcuts"].as_array().unwrap().iter().all(|row| row["app"] == "ChatGPT"));
        let press = s.call("lookup", &json!({ "keys": "press ctrl" })).unwrap();
        assert_eq!(press["used_by"].as_array().unwrap().len(), 1);
        assert_eq!(press["used_by"][0]["action"], "Toggle dictation");
    }

    #[test]
    fn lookup_and_free_keys() {
        let mut s = session(sample());
        let v = s.call("lookup", &json!({ "keys": "ctrl option f" })).unwrap();
        assert_eq!(v["used_by"].as_array().unwrap().len(), 2);
        assert_eq!(v["free"], false);
        let v = s.call("free_keys", &json!({ "modifiers": "ctrl opt", "near": "f" })).unwrap();
        let free: Vec<&str> = v["free"].as_array().unwrap().iter().map(|x| x["typed"].as_str().unwrap()).collect();
        assert!(!free.contains(&"ctrl opt f"));
        assert!(free.len() > 30);
        assert!(s.call("lookup", &json!({ "keys": "cmd banana" })).is_err());
        assert!(s.call("free_keys", &json!({ "modifiers": "cmd banana" })).is_err());
    }

    #[test]
    fn list_filters() {
        let mut s = session(sample());
        let v = s.call("list_shortcuts", &json!({ "works": "system-wide" })).unwrap();
        assert_eq!(v["total"], 2);
        let v = s.call("list_shortcuts", &json!({ "app": "mail", "text": "arch" })).unwrap();
        assert_eq!(v["total"], 1);
        assert_eq!(v["shortcuts"][0]["keyclash_can_change"], "macOS App Shortcuts (NSUserKeyEquivalents)");
    }

    #[test]
    fn propose_refuses_a_shortcut_without_a_writer() {
        let mut s = session(sample());
        let id = shortcut_id(&sample()[0]);
        let e = s.call("propose_change", &json!({ "id": id, "new_keys": "ctrl opt r" })).unwrap_err();
        assert!(e.contains("cannot change this shortcut itself"));
        assert!(s.call("propose_change", &json!({ "id": "k0", "new_keys": "ctrl opt r" })).unwrap_err().contains("No shortcut"));
    }

    #[test]
    fn every_tool_has_a_spec() {
        let specs = tool_specs();
        let names: Vec<&str> = specs.as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, TOOL_NAMES);
        for t in specs.as_array().unwrap() {
            assert_eq!(t["inputSchema"]["type"], "object");
        }
    }

    #[test]
    fn modifier_text() {
        assert_eq!(parse_mods("ctrl opt").unwrap(), CTRL | OPT);
        assert_eq!(parse_mods("⌃⌥⌘").unwrap(), CTRL | OPT | CMD);
        assert_eq!(parse_mods("cmd+shift").unwrap(), CMD | SHIFT);
        assert!(parse_mods("").is_err());
        assert!(parse_mods("ctrl 🐈").is_err());
    }

    #[test]
    fn report_contains_inventory_free_keys_and_source_limits() {
        let s = session(sample());
        let report = report_json(&s.scan, true, true);
        assert_eq!(report["schema_version"], 1);
        assert_eq!(report["all_shortcuts"].as_array().unwrap().len(), 3);
        assert!(!report["free_keys"].as_array().unwrap().is_empty());
        assert_eq!(report["deep_scan_requested"], true);
        assert_eq!(report["live_menus_read"], false);
        let free = free_with(CTRL | OPT, &HashSet::new(), Some("f"));
        assert!(free.contains(&Trigger::combo(CTRL | OPT, "F")));
    }

    #[test]
    fn malformed_arguments_return_actionable_errors() {
        let mut s = session(sample());
        for args in [json!(null), json!([]), json!({"limit": -1}), json!({"works": "global"}), json!({"app": false}), json!({"limti": 5})] {
            assert!(s.call("list_shortcuts", &args).is_err(), "{args}");
        }
    }

    #[test]
    fn lookup_uses_the_same_free_rule_as_free_keys() {
        let keys = Trigger::combo(CTRL | OPT, "J");
        let list = vec![
            Shortcut::new("Tool", "Mode action", keys.clone(), Scope::Mode, Origin::ConfigFile),
            Shortcut::new("Tool", "Disabled action", keys, Scope::Global, Origin::AppSettings).enabled(false),
        ];
        let mut s = session(list);
        let lookup = s.call("lookup", &json!({"keys":"ctrl opt j"})).unwrap();
        assert_eq!(lookup["free"], true);
        assert_eq!(lookup["used_by"].as_array().unwrap().len(), 2);
        let free = s.call("free_keys", &json!({"modifiers":"ctrl opt", "near":"j"})).unwrap();
        assert!(free["free"].as_array().unwrap().iter().any(|v| v["typed"] == "ctrl opt j"));
    }
}
