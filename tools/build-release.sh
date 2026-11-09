#!/bin/bash
# Builds the x86_64 Linux release tarball in a Debian 12 container, so the binary
# asks for an older glibc than a rolling distro would give it. Usage: build-release.sh OUTDIR
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=$(mkdir -p "$1" && cd "$1" && pwd)

podman run --rm -v "$PWD:/src:ro" -v "$OUT:/out" docker.io/library/debian:12 bash -euc '
  apt-get update -qq
  apt-get install -y -qq --no-install-recommends build-essential pkg-config curl ca-certificates \
    python3 libasound2-dev libfontconfig1-dev libfreetype6-dev libxkbcommon-dev >/dev/null
  curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal >/dev/null
  . "$HOME/.cargo/env"
  rustc --version
  cd /src
  CARGO_TARGET_DIR=/tmp/target cargo build --release --locked
  cp /tmp/target/release/redlight /out/redlight.bin
'

tools/package-linux.sh "$OUT/redlight.bin" "$OUT"
rm "$OUT/redlight.bin"
