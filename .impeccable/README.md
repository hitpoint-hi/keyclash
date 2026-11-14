# Impeccable for Keyclash

Run Impeccable from the `app/` directory. `PRODUCT.md` records the confirmed product.
`DESIGN.md` records the incumbent native interface. `.impeccable/design.json` supplies reference previews of its primitives.

## Setup

1. Install the shared Impeccable skill through Skills Manager if it is absent from `~/.codex/skills/impeccable`.
2. Run `scripts/setup-design.sh` from this repository.
3. Restart the agent session so Codex loads `.codex/hooks.json`.

The setup script links the shared skill into a Git-ignored local path. It does not copy the skill into the repository.
Commit `.codex/hooks.json`, `.impeccable/config.json`, the product record, and the design records.
Keep `.impeccable/config.local.json` on this machine. It records local hook consent.

## Native-platform limits

Impeccable 4.8.2 recognizes web, iOS, Android, and adaptive platform records. Keyclash is native macOS Rust/egui.
Its product record uses the truthful `macos` value, which the current platform resolver does not recognize.
Do not classify this app as a website or an iOS app to satisfy that resolver.

The installed hooks support web source extensions. They do not scan `.rs` files.
Do not map Rust to a JavaScript or HTML detector engine. A clean doctor report checks setup, not native UI quality.
The sidecar's HTML/CSS snippets are visual references for the design panel, not production components or proof of native behavior.

## Validation

1. Run `cargo test --locked` and `cargo build --locked`.
2. Inspect the current native app. Exercise keyboard navigation and inspect the macOS accessibility tree.
3. Check light and dark appearance after changes to colors or custom painting.
4. Run `~/.codex/skills/impeccable/scripts/impeccable doctor --json` to check setup records.

Use temporary data for interactive QA. Preserve the user's settings, favorites, usage records, and menu cache.
