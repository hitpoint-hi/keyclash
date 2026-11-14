# Agent integration verification

Date: 2026-11-12. Platform: Apple Silicon macOS. Scope: local build and agent interfaces.

## Checks

| Check | Result |
|---|---|
| `cargo test --quiet` | 119 passed, 0 failed. |
| `cargo build --release` | Passed. Binary: `target/release/keyclash`. |
| `python3 scripts/check-agent.py target/release/keyclash` | Passed with a disposable skhd file and isolated Keyclash data. |
| `git diff --check` | Passed. |
| Agent-guide language check | No violations from the local STE structural checker. |
| `cargo clippy --all-targets -- -D warnings` | Failed on existing diagnostics in untouched files. No diagnostics in changed files. |

The subprocess check verifies JSON report parsing, CLI failures, MCP initialization, tool discovery,
proposal diffs, no target write during proposal, stale apply refusal, backups, rescanning,
external-edit protection during undo, and exact restoration. It also verifies proposals,
apply, and undo across independent CLI processes. All shortcut writes use a temporary skhd fixture.

Unit tests cover TOML edit checks, key spelling, another keyboard layout, menu-path refusal,
file locking, corrupt history, recovery before and after target writes, symlinks, and Unix permissions.

## Review

The Standards review found a menu-path refusal gap. The writer now refuses stored menu paths,
including when the selected live item provides only the leaf title. A regression test covers that case.
The review also found duplicate parsers. Scan and writer validation now share the source parsers.

The Spec review found inconsistent free-key results. Lookup now uses the same enabled/scope rule
as free-key suggestions. A regression test covers disabled and special-mode shortcuts.

## Limits

No running app reload or physical shortcut activation was tested. App menu writes received
parsing and recovery logic checks. They did not receive a live preference-write or app-restart check.
The initial transport check used a local subprocess client. The later Claude connection check
below adds real client evidence. The installed Keyclash.app and release package were not replaced.

Unread sources remain explicit in JSON output. Stored config results do not establish runtime activation.

## Claude connection check

Completed 2026-11-12 after the local checks above.

- Installed the tested release binary at `~/Library/Application Support/Keyclash/bin/keyclash`.
- Added the local stdio server to Claude Desktop's `claude_desktop_config.json` and Claude Code's user configuration.
- Preserved existing settings. Backups are in `~/Library/Application Support/Keyclash/connection-backups/20261112T232215Z/`.
- Restarted Claude Desktop. Its connector menu lists `keyclash` as enabled.
- `claude mcp get keyclash` reports `Connected` with user scope.

[Claude Desktop test chat](https://claude.ai/chat/c483ba54-0701-40b5-ba4b-2df4dfaeb567)
called `summary`, `lookup`, and `free_keys`. Claude reported 3,139 shortcuts across
84 apps and tools, macOS as the owner of Command-Shift-Space, and Control-Option-Command
with G, H, and V as three free combinations. Direct queries to the installed binary
returned the same results. Claude displayed a `Used Keyclash` activity and named the
three tools in its response. Desktop's MCP log confirms initialization and tool discovery,
but does not include the tool-call payloads.

This Claude test was read-only. It did not remap personal shortcuts. The temporary-config
write/undo check above remains the evidence for remapping. Accessibility permission was off
for the server process, so live-menu reads and physical shortcut activation remain unverified.

The Desktop test establishes real model tool execution. Claude Code received a successful
MCP health check, without a separate model conversation. Neither connection needs an API key.

## Accessibility and live-menu check

Completed 2026-11-12 through the same Claude Desktop test chat.

macOS listed separate `Keyclash` app and lowercase `keyclash` executable entries.
The app entry was already enabled. The executable entry was disabled.
Enabled the executable entry after the user completed macOS Touch ID authentication.
Claude's existing server still reported no permission. Fully quitting and reopening
Claude Desktop started a new server, which then read live menus successfully.

Claude called `rescan` with `deep: true`, `summary`, and `list_shortcuts` for Finder.
The final rescan and summary both reported:

| Field | Result |
|---|---|
| `accessibility_permission` | `true` |
| `live_menus_read` | `true` |
| `live_menu_apps` | `26` |
| `shortcuts` | `3171` |
| Live-menu source count | `1104` |
| Live-menu source status | `26 running apps read` |

All 104 Finder entries had `found_in: "Live menu"`. Examples:

| Action | Keys | Menu path |
|---|---|---|
| New Folder | Shift-Command-N | File > New Folder |
| Copy as Pathname | Option-Command-C | Edit > Copy as Pathname |
| Go to Folder… | Shift-Command-G | Go > Go to Folder… |

The normal menu cache received a fresh update at `2026-11-12T23:31:05Z`.
A separate isolated scan also read live menus from 26 apps. Its count was 1101 before
the Claude restart. Counts depend on the running apps and their current menus.

No shortcut bindings changed. This establishes permission and live-menu discovery
through the actual Claude connection. Physical shortcut activation remains a separate check.
At the end of the Accessibility check, Full Disk Access was unchanged. That scan still reported 19 blocked sandboxed app settings.


## Full Disk Access check

Completed 2026-11-12 after the user explicitly approved this permission.

macOS listed separate `Keyclash` app and lowercase `keyclash` executable entries.
The app already had Full Disk Access. Enabled it for the executable used by the MCP server,
then fully quit and reopened Claude Desktop before the test.

Through the same Claude test chat, `rescan` with `deep: true` and `summary` reported:

| Field or source | Result |
|---|---|
| Sandboxed app settings | 8 shortcuts |
| Sandboxed source status | `50 app containers read` |
| Blocked settings | No blocked files reported by the scan |
| `shortcuts` | `3175` |
| `accessibility_permission` | `true` |
| `live_menus_read` | `true` |
| `live_menu_apps` | `26` |

Claude also queried Maccy and macshot. Each returned four rows from App settings.
Examples include Maccy's Popup on Shift-Command-X and macshot's Quick Capture on
Shift-Command-G. The scan preserved the disabled flags on macshot's Capture Area and Record Area.
No shortcut bindings changed. `applied_changes` remained zero.

An isolated direct scan independently returned the same sandboxed source result:
8 shortcuts and `50 app containers read`. Total shortcut and live-menu counts can differ
between scans because they depend on the open apps and their current menus.


## Claude remap and undo check

Completed 2026-11-12 through the same Claude Desktop test chat.
The test used a separate `keyclash_test` connector and a disposable skhd configuration.
Its `KEYCLASH_DATA_DIR` and `XDG_CONFIG_HOME` pointed into:
`~/Library/Application Support/Keyclash/agent-tests/20261112T233703Z/`.
The normal connector configuration remained unchanged.

Claude completed these steps:

1. Found the fixture by its exact id and config path.
2. Proposed Control-Option-F18 to Control-Option-F19, plan `p059c808643`.
3. Applied the approved plan, then reported the F19 binding and one applied change.
4. Called the test connector's `undo_last`, then reported the original F18 binding and zero applied changes.

Independent file checks established that proposal did not change the fixture.
After apply, the fixture contained the planned F19 binding and its backup matched the original bytes.
After undo, the complete fixture matched the original bytes, including comments and modifier order.
Its restored SHA256 was `c7d1a9971a797203f414bf524fa74821606fa3518dc9f244a747809f777c3dd2`.
The original shortcut id `k3842baf15a` returned after undo.

Fingerprints of the monitored personal configuration files and the normal connector's undo history
matched their starting state after apply and after undo. The test did not reload skhd or activate
a new binding in a running shortcut daemon. This verifies real Claude configuration writes and undo,
not physical key activation.

Removed `keyclash_test` from the Desktop configuration after restoration. Its removal preserved
the original configuration content. Restarted Claude and checked its connector list: normal `keyclash`
remained enabled, and the temporary connector was absent. Original, applied, and restored fixture
snapshots, the plan, the apply record, and an evidence manifest remain in the test directory.
