#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
VERSION="0.3.0"
APP_DIR="$ROOT/dist/Douri.AppDir"
OUT_IMG="$ROOT/dist/Douri-${VERSION}-x86_64.AppImage"

echo "==> Building Douri AppImage v${VERSION}..."
rm -rf "$APP_DIR" "$OUT_IMG"
mkdir -p "$APP_DIR/usr/bin"
mkdir -p "$APP_DIR/usr/share/applications"
mkdir -p "$APP_DIR/usr/share/icons/hicolor/512x512/apps"
mkdir -p "$APP_DIR/usr/share/mime/packages"

# 1. Copy release binary
cp "$ROOT/target/release/douri" "$APP_DIR/usr/bin/douri"
chmod +x "$APP_DIR/usr/bin/douri"

# 2. Copy desktop file, icon, and mime
cp "$ROOT/dist/linux/io.github.triadwoozie.douri.desktop" "$APP_DIR/usr/share/applications/io.github.triadwoozie.douri.desktop"
cp "$ROOT/dist/linux/io.github.triadwoozie.douri.png" "$APP_DIR/usr/share/icons/hicolor/512x512/apps/io.github.triadwoozie.douri.png"
cp "$ROOT/dist/linux/douri-mime.xml" "$APP_DIR/usr/share/mime/packages/douri.xml"

# Root level AppDir requirements
cp "$ROOT/dist/linux/io.github.triadwoozie.douri.desktop" "$APP_DIR/io.github.triadwoozie.douri.desktop"
cp "$ROOT/dist/linux/io.github.triadwoozie.douri.png" "$APP_DIR/io.github.triadwoozie.douri.png"
ln -sf io.github.triadwoozie.douri.png "$APP_DIR/.DirIcon"

# AppRun script
cat << 'RUN' > "$APP_DIR/AppRun"
#!/bin/sh
SELF=$(readlink -f "$0")
HERE=${SELF%/*}
export PATH="${HERE}/usr/bin:${PATH}"
export XDG_DATA_DIRS="${HERE}/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
exec "${HERE}/usr/bin/douri" "$@"
RUN
chmod +x "$APP_DIR/AppRun"

# 3. Build AppImage using appimagetool
ARCH=x86_64 VERSION="$VERSION" APPIMAGE_EXTRACT_AND_RUN=1 appimagetool "$APP_DIR" "$OUT_IMG"
chmod +x "$OUT_IMG"
echo "==> Successfully created $OUT_IMG"
ls -lh "$OUT_IMG"
