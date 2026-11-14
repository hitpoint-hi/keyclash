# Native app design work

Run Rust and Impeccable commands from this directory, which contains the Git repository and `Cargo.toml`.
Read `PRODUCT.md` and `DESIGN.md` before changing the interface.

For Impeccable setup and its native-platform limits, read `.impeccable/README.md`.
Use `scripts/setup-design.sh` to link the shared skill and repair Codex hooks on a new checkout.

Validate egui changes with Rust tests, the rendered native app, and its accessibility tree.
Impeccable's web detector does not validate Rust/egui. Report that limit when describing detector results.
