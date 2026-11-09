#!/bin/sh
# Installs into user locations only: ~/.local/bin and ~/.local/share (same
# layout as Plaza's install.sh). Uses the prebuilt binary beside this script if
# there is one (the release tarball), otherwise builds from source.
set -eu
cd "$(dirname "$0")"
if [ -x ./redlight ] && [ -f ./redlight ]; then
    bin=./redlight
else
    cargo build --release
    bin="${CARGO_TARGET_DIR:-target}/release/redlight"
fi
share="$HOME/.local/share"
install -Dm755 "$bin" "$HOME/.local/bin/redlight"
install -Dm644 assets/redlight.svg "$share/icons/hicolor/scalable/apps/redlight.svg"
install -Dm644 assets/redlight-256.png "$share/icons/hicolor/256x256/apps/redlight.png"
install -Dm644 assets/redlight-64.png "$share/icons/hicolor/64x64/apps/redlight.png"
mkdir -p "$share/applications"
sed "s|@BIN@|$HOME/.local/bin/redlight|" assets/redlight.desktop > "$share/applications/redlight.desktop"
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
command -v update-desktop-database >/dev/null && update-desktop-database "$share/applications" >/dev/null 2>&1 || true
echo "Installed. Launch Red Light from the app menu, or run: redlight"
