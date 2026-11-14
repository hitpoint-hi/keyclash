#!/bin/bash
# Make dist/Keyclash-<version>.zip: one app for Apple Silicon and Intel Macs.
# The app has an ad-hoc signature. macOS asks each person to allow it one time.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

cargo test --quiet
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

rm -rf dist
mkdir -p dist/build
lipo -create target/aarch64-apple-darwin/release/keyclash target/x86_64-apple-darwin/release/keyclash -output dist/build/keyclash
./dist/build/keyclash --bundle dist
rm -rf dist/build
codesign --verify --deep --strict dist/Keyclash.app
(cd dist && ditto -c -k --keepParent Keyclash.app "Keyclash-$version.zip")
echo "Made dist/Keyclash-$version.zip ($(lipo -archs dist/Keyclash.app/Contents/MacOS/keyclash))"
