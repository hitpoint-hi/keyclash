# Product

<!-- impeccable:product-schema 1 -->

## Platform

macos

This is a native macOS app built with Rust, egui, and eframe. It has no web view.
Impeccable 4.8.2 has no macOS platform guide or Rust UI detector. Its platform resolver
returns no recognized platform for `macos`. Use the native source and rendered app as evidence.

## Users

Keyboard-focused Mac users who want to find shortcuts, resolve clashes, and understand their shortcut use.
Bobby confirmed this audience and the local-only product on 2026-11-12.

## Product Purpose

Keyclash inventories keyboard shortcuts from installed apps, macOS settings, and supported tool configurations.
It distinguishes system-wide clashes from normal sharing between apps.

## Operating Context

The app runs on the user's Mac. Accessibility permission enables live menu reads and the global panel trigger.
Some sandboxed settings require Full Disk Access. Files remain useful when live access is unavailable.
The shortcut panel follows the foreground app. The main window provides inventory, lookup, conflicts, and usage views.

## Capabilities and Constraints

- Shortcut discovery depends on readable sources. Unknown shortcuts must remain explicit.
- Usage counting is opt-in. Records contain daily counts, key combinations, and the foreground app. They contain no typed text.
- Key observations do not prove which action ran. Several possible actions remain ambiguous and count once.
- Historical usage records remain intact when matching rules change.
- Remembered menus apply only to the same installed app identity. Opening an app refreshes its live menus when permission permits.
- Keyclash has no network code. Distribution and release checks remain separate from local tests.

## Brand Commitments

Preserve the Keyclash name, the existing icon in `assets/`, and the current compact interface.
Bobby confirmed preservation of this interface on 2026-11-12. This task does not establish a new visual direction.

## Product Principles

1. Keep observations local and explain what is stored.
2. Show uncertainty instead of inventing an action or shortcut owner.
3. Put actionable conflicts before potential conflicts involving closed tools.
4. Support keyboard navigation and expose meaningful names and states to macOS accessibility tools.

## Evidence on Hand

`README.md` documents the product and build commands. `src/theme.rs` defines the visual system.
Tests cover shortcut parsing, matching, menu persistence, conflict classification, and accessible custom controls.
Screenshots in `shots/` are local captures and are not in the repository. Verify changes against the current build.
