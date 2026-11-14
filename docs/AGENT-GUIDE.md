# Control Keyclash from an agent

Build the binary with `cargo build --release`. Use its absolute path in your agent client.
The CLI and MCP server run without a window. They do not start the key counter.

## JSON and CLI tools

```sh
./target/release/keyclash --report --json > shortcuts.json
./target/release/keyclash --tool help
./target/release/keyclash --tool conflicts '{"limit":10}'
./target/release/keyclash --tool list_shortcuts '{"app":"AeroSpace"}'
./target/release/keyclash --tool lookup '{"keys":"ctrl opt f"}'
./target/release/keyclash --tool free_keys '{"modifiers":"ctrl opt","near":"f"}'
```

The report contains `schema_version`, source status, conflict counts, `conflict_list`,
`all_shortcuts`, and free keys grouped by modifier keys. Tool results use JSON on stdout.
CLI errors use JSON on stderr and exit with status 1. `--report` and `--report --dump`
retain their text output. Add `--deep` for Electron code, sandboxed settings, and live menus.
Live menus need Accessibility permission for the process that starts Keyclash.
A requested deep scan does not establish that macOS allowed each source. Check `sources`.

## Connect an MCP client

Add this entry to your client's MCP server configuration. Replace the command with the
absolute path of your built binary or the executable inside an updated Keyclash.app.

```json
{
  "mcpServers": {
    "keyclash": {
      "command": "/absolute/path/to/keyclash",
      "args": ["--mcp"]
    }
  }
}
```

Start with the normal scan. Use `rescan` with `{"deep":true}` when you need more sources.
The server uses local stdin/stdout, with one JSON-RPC message per line.
It opens no network port and requires no API key.
The handshake negotiates the client's supported protocol version before tool calls.

## Remap one shortcut

1. Call `conflicts` or `list_shortcuts`. Select a current shortcut `id` with `keyclash_can_change`.
2. Call `free_keys` or `lookup` to select new keys.
3. Call `propose_change` with `{"id":"<shortcut id>","new_keys":"ctrl opt j"}`.
4. Show the person the diff, predicted conflicts, other users of the keys, and reload instructions. Obtain approval for this plan.
5. Call `apply_change` with `{"plan_id":"<plan id>"}`. Follow `after_apply`, then call `rescan`.

`undo_last` restores the last change. Reload the owning tool after undo too.
If a tool uses CLI commands, pass the same tool names and JSON arguments to `--tool`.
Plans persist across processes, so propose and apply can use separate CLI calls.
A proposal writes a local plan record. It does not change the shortcut configuration.

## Supported writes

| Source | Support and limits |
|---|---|
| AeroSpace | Rename one key in a normal `[mode.<name>.binding]` section. Preserve comments and commands. Refuse custom key mappings, non-QWERTY presets, and unsupported syntax. |
| OmniWM | Change one hotkey binding in TOML. Refuse ambiguous inline rows or duplicate keys. |
| skhd | Change one binding with an exact command match. Preserve its mode prefix. Refuse duplicate keys and line continuations. |
| App menu | Set an `NSUserKeyEquivalents` override by exact title. Refuse detected sandboxed preferences and stored menu paths. Restart the app. Identical menu titles can affect several items. |
| Other sources | Return manual instructions. Karabiner, system hotkeys, and private app settings remain read-only. |

Estimated and disabled bindings need manual inspection. Supported writers can still refuse
particular files or keys when they cannot check the change.

## Check the result

A successful write checks the stored configuration and scans again. It does not prove
that the owning app loaded the new keys or ran the action. Use `after_apply`, then test
the shortcut in the owning app. Menu caches and installed menu files can retain old keys
until an app restart and a permitted live scan.

`conflicts_after` predicts the selected row's conflicts. It does not simulate app behavior.
Free keys mean no enabled shortcut outside special modes in the current scan uses them.
Unread sources, special modes, and changes after the scan can affect that result.
`usage` reports the last 30 days from the opt-in counter. A press does not prove an action ran.

## Backups and recovery

Keyclash stores proposals, history, and file backups under its data folder's `agent/`
subdirectory. It saves recovery data before a write and serializes agent writes with a file lock.
Apply refuses changes since proposal. Undo refuses later edits that differ from both saved versions.
If a write stops partway, call `undo_last` before another apply. When the write did not land,
apply removes its own recovery record and says that Keyclash wrote nothing.
When an unfinished apply left the target unchanged or edited by hand, `undo_last` restores nothing,
removes the record, and answers with `cleared` and the backup path.
If external edits prevent recovery of a finished change, inspect the backup path in the error.
Preserve the history files.

For tests, `KEYCLASH_DATA_DIR` selects a separate Keyclash data folder. It does not redirect
third-party config files. Use disposable config fixtures for write tests.

## Privacy

Keyclash itself sends no network requests. A connected agent receives tool results, which
can include app names, config paths, commands, shortcut use counts, and configuration diffs.
Your agent client's provider and privacy settings control any further transmission.

Protocol references: [stdio transport](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
and [initialization](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle).
