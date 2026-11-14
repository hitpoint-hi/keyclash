# Keyclash eframe patch

Source: crates.io `eframe` 0.37.0, licensed under MIT OR Apache-2.0.
The source comes from the published crate. Its upstream commit is `3c6eb8261c3293ad9c7ea1ed58487e038860ebef`.
The license files come from that upstream commit.

The WGPU backend initializes AccessKit only for the root window.
Child windows therefore render controls without exposing them to macOS accessibility tools.
The Keyclash patch passes the existing event proxy to child-window creation and initializes AccessKit there.
It creates the window hidden, as AccessKit requires, then restores the requested visibility after initialization.
For inactive macOS child windows, it uses AppKit `orderFrontRegardless` to show the window without making it key.
Winit's `set_visible(true)` calls `makeKeyAndOrderFront`, which steals focus even when the builder specifies `active = false`.
It covers immediate and deferred child windows through their shared initialization function.
The Glow backend is unchanged. Keyclash uses WGPU.

Compare `src/native/wgpu_integration.rs` with the published crate to inspect the patch.
Remove the Cargo patch when an upstream release includes this initialization.

Native validation must check the Settings window tree, keyboard focus, close, and reopen.
