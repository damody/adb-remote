#!/usr/bin/env bash
set -euo pipefail

VERSION="${VERSION:-0.1.0}"
TARGET="${TARGET:-x86_64-unknown-linux-gnu}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$ROOT/dist"
PKG="$DIST/adb-remote-server-${VERSION}-linux-x64"

cd "$ROOT"
cargo build --release -p adb-remote-server --target "$TARGET"

rm -rf "$PKG"
mkdir -p "$PKG/bin" "$PKG/docs" "$PKG/systemd"
cp "target/$TARGET/release/adb-remote-server" "$PKG/bin/"
cp "docs/server-linux.md" "$PKG/docs/"
cp "packaging/linux/adb-remote-server.service" "$PKG/systemd/"

tar -C "$DIST" -czf "$PKG.tar.gz" "$(basename "$PKG")"
echo "Wrote $PKG.tar.gz"
