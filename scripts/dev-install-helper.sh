#!/bin/bash
# Installs a development build of yaist-helper as root-owned
# /usr/local/libexec/yaist/yaist-helper with its polkit policy, so pkexec
# keeps the authentication for a few minutes instead of asking every time.
# Re-run after changing the helper. Undo with --remove.
set -euo pipefail
cd "$(dirname "$0")/.."
DEST=/usr/local/libexec/yaist/yaist-helper
POLICY=/usr/share/polkit-1/actions/org.opensuse.yaist.policy
if [ "${1:-}" = "--remove" ]; then
    sudo rm -f "$DEST" "$POLICY"
    echo "removed $DEST and $POLICY"
    exit 0
fi
[ -x target/debug/yaist-helper ] || { echo "build first: cargo build (or scripts/build-in-container.sh build)"; exit 1; }
sudo install -D -m 0755 -o root -g root target/debug/yaist-helper "$DEST"
sed "s|/usr/libexec/yaist/yaist-helper|$DEST|" packaging/org.opensuse.yaist.policy | sudo install -D -m 0644 /dev/stdin "$POLICY"
echo "installed $DEST and $POLICY"
