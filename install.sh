#!/bin/sh
# Installs into user locations only: ~/.local/bin and ~/.local/share (same
# layout as Plaza's install.sh). Uses the prebuilt binary beside this script if
# there is one (the release tarball), otherwise builds from source.
set -eu
cd "$(dirname "$0")"
if [ -x ./backroom ] && [ -f ./backroom ]; then
    bin=./backroom
else
    cargo build --release
    bin="${CARGO_TARGET_DIR:-target}/release/backroom"
fi
share="$HOME/.local/share"
install -Dm755 "$bin" "$HOME/.local/bin/backroom"
install -Dm644 assets/backroom.svg "$share/icons/hicolor/scalable/apps/backroom.svg"
install -Dm644 assets/backroom-256.png "$share/icons/hicolor/256x256/apps/backroom.png"
install -Dm644 assets/backroom-64.png "$share/icons/hicolor/64x64/apps/backroom.png"
mkdir -p "$share/applications"
sed "s|@BIN@|$HOME/.local/bin/backroom|" assets/backroom.desktop > "$share/applications/backroom.desktop"
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
command -v update-desktop-database >/dev/null && update-desktop-database "$share/applications" >/dev/null 2>&1 || true
echo "Installed. Launch Backroom from the app menu, or run: backroom"
