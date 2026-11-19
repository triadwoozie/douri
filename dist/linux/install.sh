#!/bin/sh
# Installs Dōri for the current user (no root needed). Undo with: ./install.sh --uninstall
set -e
here=$(cd "$(dirname "$0")" && pwd)
bin="$HOME/.local/bin"
apps="$HOME/.local/share/applications"
icons_root="$HOME/.local/share/icons/hicolor"
icons512="$icons_root/512x512/apps"
icons256="$icons_root/256x256/apps"
mime="$HOME/.local/share/mime"

if [ "$1" = "--uninstall" ]; then
  rm -f "$bin/douri" \
        "$apps/io.github.triadwoozie.douri.desktop" \
        "$icons512/io.github.triadwoozie.douri.png" \
        "$icons256/io.github.triadwoozie.douri.png" \
        "$mime/packages/douri.xml"
  command -v update-desktop-database >/dev/null && update-desktop-database "$apps" || true
  command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -f -t "$icons_root" || true
  echo "Dōri removed."
else
  # Locate douri binary: in current directory (tarball) or target/release/douri (git repo)
  douri_bin=""
  if [ -f "$here/douri" ]; then
    douri_bin="$here/douri"
  elif [ -f "$here/../../target/release/douri" ]; then
    douri_bin="$here/../../target/release/douri"
  elif [ -f "$here/../target/release/douri" ]; then
    douri_bin="$here/../target/release/douri"
  fi

  if [ -z "$douri_bin" ]; then
    echo "Error: douri executable not found in $here or target/release/. Build first with 'cargo build --release --bin douri'." >&2
    exit 1
  fi

  mkdir -p "$bin" "$apps" "$icons512" "$icons256" "$mime/packages"
  install -m 755 "$douri_bin" "$bin/douri"

  desktop_file=""
  if [ -f "$here/io.github.triadwoozie.douri.desktop" ]; then
    desktop_file="$here/io.github.triadwoozie.douri.desktop"
  elif [ -f "$here/linux/io.github.triadwoozie.douri.desktop" ]; then
    desktop_file="$here/linux/io.github.triadwoozie.douri.desktop"
  fi
  if [ -n "$desktop_file" ]; then
    install -m 644 "$desktop_file" "$apps/io.github.triadwoozie.douri.desktop"
  fi

  # Install 512x512 icon
  icon512=""
  if [ -f "$here/io.github.triadwoozie.douri.png" ]; then
    icon512="$here/io.github.triadwoozie.douri.png"
  elif [ -f "$here/linux/io.github.triadwoozie.douri.png" ]; then
    icon512="$here/linux/io.github.triadwoozie.douri.png"
  fi
  if [ -n "$icon512" ]; then
    install -m 644 "$icon512" "$icons512/io.github.triadwoozie.douri.png"
  fi

  # Install 256x256 icon
  icon256=""
  if [ -f "$here/icon-256.png" ]; then
    icon256="$here/icon-256.png"
  elif [ -f "$here/../../crates/prowave-app/assets/icon-256.png" ]; then
    icon256="$here/../../crates/prowave-app/assets/icon-256.png"
  elif [ -f "$here/../crates/prowave-app/assets/icon-256.png" ]; then
    icon256="$here/../crates/prowave-app/assets/icon-256.png"
  elif [ -n "$icon512" ]; then
    icon256="$icon512"
  fi
  if [ -n "$icon256" ]; then
    install -m 644 "$icon256" "$icons256/io.github.triadwoozie.douri.png"
  fi

  mime_file=""
  if [ -f "$here/douri-mime.xml" ]; then
    mime_file="$here/douri-mime.xml"
  elif [ -f "$here/linux/douri-mime.xml" ]; then
    mime_file="$here/linux/douri-mime.xml"
  fi
  if [ -n "$mime_file" ]; then
    install -m 644 "$mime_file" "$mime/packages/douri.xml"
    command -v update-mime-database >/dev/null && update-mime-database "$mime" || true
  fi

  command -v update-desktop-database >/dev/null && update-desktop-database "$apps" || true
  command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -f -t "$icons_root" || true

  echo "Installed Dōri to $bin/douri."
  echo "Desktop entry and icons registered. Start Dōri from your app menu or terminal: douri"
  case ":$PATH:" in *":$bin:"*) ;; *) echo "Note: add $bin to your PATH to run it directly from a terminal." ;; esac
fi
