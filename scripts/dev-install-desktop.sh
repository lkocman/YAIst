#!/bin/bash
# Installs the YAiST icon and a desktop entry for the development build into
# ~/.local/share (no root), so the desktop shows the icon; on Wayland that is
# the only way. Undo with --remove.
set -euo pipefail
cd "$(dirname "$0")/.."
APPS="$HOME/.local/share/applications"
ICONS="$HOME/.local/share/icons/hicolor"
if [ "${1:-}" = "--remove" ]; then
    rm -f "$APPS/org.opensuse.yaist.desktop" "$ICONS"/*/apps/org.opensuse.yaist.png "$ICONS"/*/apps/org.opensuse.yaist.svg
    echo "removed"
    exit 0
fi
for icon in assets/icons/hicolor/*/apps/org.opensuse.yaist.*; do
    install -D -m 0644 "$icon" "$ICONS/${icon#assets/icons/hicolor/}"
done
sed "s|^Exec=.*|Exec=$PWD/target/debug/yaist-gui|" packaging/org.opensuse.yaist.desktop | install -D -m 0644 /dev/stdin "$APPS/org.opensuse.yaist.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$ICONS" 2>/dev/null || true
echo "installed: YAiST is in the app menu, with its icon"
