#!/bin/bash
# Builds Red Light.app from an arm64 and an x86_64 binary, ad-hoc signs it, and wraps
# it in a .dmg. Runs on macOS: package-macos.sh ARM64_BIN X86_64_BIN OUTDIR
set -euo pipefail
cd "$(dirname "$0")/.."
ARM=$1; X86=$2; OUT=$(mkdir -p "$3" && cd "$3" && pwd)
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
NAME="redlight-$VERSION-macos-universal"
STAGE=$(mktemp -d); APP="$STAGE/Red Light.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$STAGE/Licenses"
lipo -create -output "$APP/Contents/MacOS/redlight" "$ARM" "$X86"
lipo -info "$APP/Contents/MacOS/redlight"
cp assets/redlight.icns "$APP/Contents/Resources/redlight.icns"
sed "s/@VERSION@/$VERSION/g" tools/dist/Info.plist > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"
# With MACOS_SIGN_IDENTITY ("Developer ID Application: ...") in the keychain: a real
# signature with the hardened runtime (Red Light only plays audio, so it needs no
# entitlements). Without it: an ad-hoc signature, so Gatekeeper asks once (see READ ME
# FIRST); Apple Silicon refuses to run a binary with no signature at all.
if [ -n "${MACOS_SIGN_IDENTITY:-}" ]; then
  codesign --force --options runtime --timestamp --sign "$MACOS_SIGN_IDENTITY" "$APP"
else
  codesign --force --deep --sign - "$APP"
fi
codesign --verify --strict --verbose=2 "$APP"
python3 -I tools/third-party-licenses.py "$STAGE/Licenses/THIRD-PARTY-LICENSES.txt" aarch64-apple-darwin x86_64-apple-darwin
cp README.md NOTICE LICENSE-MIT LICENSE-APACHE "$STAGE/Licenses/"
cp -r LICENSES/. "$STAGE/Licenses/"
cp tools/dist/READ-ME-FIRST-macos.txt "$STAGE/READ ME FIRST.txt"
ln -s /Applications "$STAGE/Applications"
rm -f "$OUT/$NAME.dmg"
hdiutil create -volname "Red Light $VERSION" -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$OUT/$NAME.dmg" >/dev/null
if [ -n "${MACOS_SIGN_IDENTITY:-}" ]; then
  codesign --force --timestamp --sign "$MACOS_SIGN_IDENTITY" "$OUT/$NAME.dmg"
fi
# Notarize with an App Store Connect API key (NOTARY_KEY_PATH/_ID/_ISSUER), then staple
# the ticket so the first open works offline too.
if [ -n "${NOTARY_KEY_PATH:-}" ]; then
  xcrun notarytool submit "$OUT/$NAME.dmg" --key "$NOTARY_KEY_PATH" --key-id "$NOTARY_KEY_ID" \
    --issuer "$NOTARY_ISSUER" --wait --timeout 30m
  xcrun stapler staple "$OUT/$NAME.dmg"
  spctl --assess --type open --context context:primary-signature --verbose=2 "$OUT/$NAME.dmg"
fi
(cd "$OUT" && shasum -a 256 "$NAME.dmg" > "$NAME.dmg.sha256" && cat "$NAME.dmg.sha256")
rm -rf "$STAGE"
