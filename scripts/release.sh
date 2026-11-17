#!/bin/bash
# Make dist/Keyclash-<version>.zip: one app for Apple Silicon and Intel Macs.
# The app is signed with Developer ID, notarized by Apple, and stapled, so macOS opens it without a warning.
#
# Needs, one time on this Mac:
#   - The "Developer ID Application" certificate and its private key in the login keychain.
#   - A notarytool profile: xcrun notarytool store-credentials keyclash --apple-id <email> --team-id 6F3C6945FS
# Set KEYCLASH_UNSIGNED=1 to make the old ad-hoc build (no Apple account needed).
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
identity="${KEYCLASH_SIGN_ID:-Developer ID Application: Robert Hunnicutt Jr (6F3C6945FS)}"
profile="${KEYCLASH_NOTARY_PROFILE:-keyclash}"

cargo test --quiet
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

rm -rf dist
mkdir -p dist/build
lipo -create target/aarch64-apple-darwin/release/keyclash target/x86_64-apple-darwin/release/keyclash -output dist/build/keyclash
./dist/build/keyclash --bundle dist
rm -rf dist/build

if [ "${KEYCLASH_UNSIGNED:-}" != "1" ]; then
  # Replace the ad-hoc signature: hardened runtime and a secure timestamp are required for notarization.
  codesign --force --options runtime --timestamp --sign "$identity" dist/Keyclash.app
  codesign --verify --deep --strict dist/Keyclash.app
  (cd dist && ditto -c -k --keepParent Keyclash.app notarize.zip)
  xcrun notarytool submit dist/notarize.zip --keychain-profile "$profile" --wait
  rm dist/notarize.zip
  # Staple the ticket so the app opens without a network check.
  xcrun stapler staple dist/Keyclash.app
  spctl --assess --type execute --verbose dist/Keyclash.app
else
  codesign --verify --deep --strict dist/Keyclash.app
fi

(cd dist && ditto -c -k --keepParent Keyclash.app "Keyclash-$version.zip" && cp "Keyclash-$version.zip" Keyclash.zip)
echo "Made dist/Keyclash-$version.zip and dist/Keyclash.zip; upload both to the release (the site links to Keyclash.zip)"
echo "Built ($(lipo -archs dist/Keyclash.app/Contents/MacOS/keyclash))"
