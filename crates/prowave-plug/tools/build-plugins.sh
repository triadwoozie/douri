#!/bin/bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Builds Pro Wave's instruments and effects as plug-ins: for each, a universal (arm64 and x86_64)
# .vst3, .clap and .component (Audio Unit v2), signed ad hoc, with LICENSE and the third-party
# notices in each bundle's Resources. Run it on a Mac with Rust (both macOS targets installed:
# `rustup target add aarch64-apple-darwin x86_64-apple-darwin`), cmake, git, python3 and the
# Xcode command-line tools:
#
#   bash crates/prowave-plug/tools/build-plugins.sh OUT [ID ...]
#
# OUT gets <Name>.vst3, <Name>.clap and <Name>.component for each plug-in, and plugins.tsv (the
# list: id, kind, name, CLAP id, AU type, subtype and manufacturer, parameters, presets). With
# no ids it builds every instrument and effect of the public build. Build folders go under
# crates/prowave-plug/target, or PLUG_TARGET_DIR. The Audio Units are made by clap-wrapper
# (fetched from GitHub at a fixed commit on the first run, with the SDKs it needs).
set -euo pipefail
if [ $# -lt 1 ]; then
  echo "usage: bash crates/prowave-plug/tools/build-plugins.sh OUT [ID ...]" >&2
  exit 1
fi
HERE="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="$(mkdir -p "$1" && cd "$1" && pwd)"
shift
ONLY=" $* "
TD="${PLUG_TARGET_DIR:-$HERE/target}"
CARGO="${CARGO:-cargo}"
MANIFEST="$HERE/Cargo.toml"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$MANIFEST" | head -1)
export MACOSX_DEPLOYMENT_TARGET=11.0
NOTICES_MD="$ROOT/public/THIRD-PARTY-NOTICES.md"
[ -f "$NOTICES_MD" ] || NOTICES_MD="$ROOT/THIRD-PARTY-NOTICES.md"
FONTS="$ROOT/crates/prowave-app/assets/fonts"
WORK="$TD/bundle-work"
mkdir -p "$WORK"

# clap-wrapper, at the commit the Audio Units were first made and checked with.
CW="$TD/clap-wrapper"
CW_REV=806bba0a5f792e3966b99c1561ab9952d3316d7c
if [ ! -d "$CW/.git" ]; then
  git clone -q https://github.com/free-audio/clap-wrapper "$CW"
fi
git -C "$CW" checkout -q "$CW_REV"
GEN="Unix Makefiles"
if command -v ninja >/dev/null; then GEN=Ninja; fi

# The list, from the engine's tables, and the Rust crates the plug-ins are built from.
"$CARGO" run -q --manifest-path "$MANIFEST" --target-dir "$TD/host" --example list >"$OUT/plugins.tsv"
"$CARGO" metadata -q --format-version 1 --manifest-path "$MANIFEST" \
  --filter-platform aarch64-apple-darwin | python3 "$HERE/tools/crates.py" >"$WORK/crates.tsv"

# Info.plist of a .vst3 or .clap bundle.
plist() { # name, bundle id
  cat <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>English</string>
    <key>CFBundleExecutable</key>
    <string>$1</string>
    <key>CFBundleIdentifier</key>
    <string>$2</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>$1</string>
    <key>CFBundleDisplayName</key>
    <string>$1</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>
    <string>$MACOSX_DEPLOYMENT_TARGET</string>
    <key>NSHumanReadableCopyright</key>
    <string>GPL-3.0-or-later</string>
    <key>NSHighResolutionCapable</key>
    <true/>
  </dict>
</plist>
PLIST
}

# LICENSE, the notices and the fonts' licences into a bundle's Resources.
resources() { # bundle, id, name
  local r="$1/Contents/Resources"
  mkdir -p "$r"
  cp "$ROOT/LICENSE" "$r/LICENSE"
  cp "$FONTS/OFL-BarlowCondensed.txt" "$FONTS/OFL-JetBrainsMono.txt" "$r/"
  python3 "$HERE/tools/notices.py" "$2" "$3" "$VERSION" "$NOTICES_MD" "$WORK/crates.tsv" \
    "$r/THIRD-PARTY-NOTICES.txt"
}

while IFS=$'\t' read -r id kind name clap_id au_type au_sub au_manu _params _presets; do
  if [ "$ONLY" != "  " ] && [[ "$ONLY" != *" $id "* ]]; then
    continue
  fi
  echo "== $name ($id, $kind)"
  # Both architectures at once, each in its own build folder.
  pids=()
  for arch in aarch64 x86_64; do
    PROWAVE_PLUG_ID="$id" "$CARGO" build -q --release --lib --manifest-path "$MANIFEST" \
      --target "$arch-apple-darwin" --target-dir "$TD/$arch" &
    pids+=($!)
  done
  for p in "${pids[@]}"; do wait "$p"; done
  lib="$WORK/$name.dylib"
  lipo -create \
    "$TD/aarch64/aarch64-apple-darwin/release/libprowave_plug.dylib" \
    "$TD/x86_64/x86_64-apple-darwin/release/libprowave_plug.dylib" -output "$lib"
  for fmt in vst3 clap; do
    b="$OUT/$name.$fmt"
    rm -rf "$b"
    mkdir -p "$b/Contents/MacOS"
    cp "$lib" "$b/Contents/MacOS/$name"
    printf 'BNDL????' >"$b/Contents/PkgInfo"
    plist "$name" "$clap_id.$fmt" >"$b/Contents/Info.plist"
    resources "$b" "$id" "$name"
    codesign --force --deep -s - "$b" 2>/dev/null
  done
  # The Audio Unit: clap-wrapper around the .clap, which goes inside it.
  B="$TD/au-build"
  # A build folder cmake made from another checkout cannot be reused.
  if [ -f "$B/CMakeCache.txt" ] && ! grep -qx "CMAKE_HOME_DIRECTORY:INTERNAL=$HERE/au" "$B/CMakeCache.txt"; then
    rm -rf "$B"
  fi
  cmake -S "$HERE/au" -B "$B" -G "$GEN" -DCMAKE_BUILD_TYPE=Release -DCLAP_WRAPPER_DIR="$CW" \
    -DCLAP_PATH="$OUT/$name.clap" -DAU_OUTPUT_NAME="$name" -DAU_TYPE="$au_type" \
    -DAU_SUBTYPE="$au_sub" -DAU_MANUFACTURER_CODE="$au_manu" -DAU_MANUFACTURER_NAME="Pro Wave" \
    -DAU_BUNDLE_ID="$clap_id.component" -DAU_VERSION="$VERSION" >/dev/null
  rm -f "$B/wrapped_auv2-build-helper"
  rm -rf "$B/$name.component"
  cmake --build "$B" --target wrapped_auv2 >/dev/null
  c="$B/$name.component"
  mkdir -p "$c/Contents/PlugIns"
  rm -rf "$c/Contents/PlugIns/$name.clap"
  cp -R "$OUT/$name.clap" "$c/Contents/PlugIns/$name.clap"
  resources "$c" "$id" "$name"
  # The name a host lists under the manufacturer: "Pro Wave: Kick", not "Pro Wave: Pro Wave Kick".
  plutil -replace AudioComponents.0.name -string "Pro Wave: ${name#Pro Wave }" "$c/Contents/Info.plist"
  codesign --force --deep -s - "$c" 2>/dev/null
  rm -rf "$OUT/$name.component"
  cp -R "$c" "$OUT/"
done <"$OUT/plugins.tsv"
echo "built into $OUT"
