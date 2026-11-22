#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
VERSION="0.3.0"
DIST_DIR="$ROOT/dist"
PKG_DIR="$DIST_DIR/douri-v${VERSION}-windows-x86_64"
OUT_ZIP="$DIST_DIR/douri-v${VERSION}-windows-x86_64.zip"

echo "==> Packaging Windows x64 release..."

mkdir -p "$PKG_DIR"
# Check if windows target binary exists
if [ -f "$ROOT/target/x86_64-pc-windows-gnu/release/douri.exe" ]; then
    cp "$ROOT/target/x86_64-pc-windows-gnu/release/douri.exe" "$PKG_DIR/douri.exe"
elif [ -f "$ROOT/target/release/douri.exe" ]; then
    cp "$ROOT/target/release/douri.exe" "$PKG_DIR/douri.exe"
else
    echo "Notice: Windows .exe not yet built. Please run:"
    echo "  cargo build --target x86_64-pc-windows-gnu --release -p douri"
    echo "or compile natively on Windows via cargo build --release -p douri."
fi

cp "$HERE/README.txt" "$PKG_DIR/README.txt"
if [ -f "$ROOT/LICENSE" ]; then
    cp "$ROOT/LICENSE" "$PKG_DIR/LICENSE"
fi

if [ -f "$PKG_DIR/douri.exe" ]; then
    (cd "$DIST_DIR" && zip -r -9 "$(basename "$OUT_ZIP")" "$(basename "$PKG_DIR")")
    echo "==> Successfully packaged: $OUT_ZIP"
    ls -lh "$OUT_ZIP"
else
    echo "Packaging prepared in $PKG_DIR (ready to bundle once .exe is generated)."
fi
