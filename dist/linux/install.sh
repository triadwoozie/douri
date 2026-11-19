#!/bin/sh
# Installs Dōri for the current user (no root needed). Undo with: ./install.sh --uninstall
set -e
here=$(cd "$(dirname "$0")" && pwd)
bin="$HOME/.local/bin"; apps="$HOME/.local/share/applications"
icons="$HOME/.local/share/icons/hicolor/512x512/apps"; mime="$HOME/.local/share/mime"
if [ "$1" = "--uninstall" ]; then
  rm -f "$bin/douri" "$apps/io.github.triadwoozie.douri.desktop" "$icons/io.github.triadwoozie.douri.png" "$mime/packages/douri.xml"
  echo "Dōri removed."
else
  mkdir -p "$bin" "$apps" "$icons" "$mime/packages"
  install -m 755 "$here/douri" "$bin/douri"
  install -m 644 "$here/io.github.triadwoozie.douri.desktop" "$apps/"
  install -m 644 "$here/io.github.triadwoozie.douri.png" "$icons/"
  install -m 644 "$here/douri-mime.xml" "$mime/packages/douri.xml"
  command -v update-mime-database >/dev/null && update-mime-database "$mime" || true
  command -v update-desktop-database >/dev/null && update-desktop-database "$apps" || true
  echo "Installed to $bin/douri. Start it from your app menu or run: douri"
  case ":$PATH:" in *":$bin:"*) ;; *) echo "Note: add $bin to your PATH to run it from a terminal." ;; esac
fi
