#!/usr/bin/env bash
# Build/install the Mira GNOME Shell extension.
#
#   ./build.sh            copy tokens, compile schemas, produce mira.zip
#   ./build.sh install    the above, then install into the user extensions dir
#
# extensions.gnome.org cannot host the daemon binary, so Mira is distributed as
# a .deb that installs the daemon + this extension together (see packaging/).

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$HERE/.." && pwd)"
UUID="mira@novabytecode.ro"

echo "==> Copying design tokens into the extension"
cp "$REPO_ROOT/design/tokens.json" "$HERE/tokens.json"

echo "==> Compiling GSettings schemas"
glib-compile-schemas "$HERE/schemas"

echo "==> Packing $UUID.zip"
( cd "$HERE" && zip -qr -FS "$HERE/$UUID.zip" \
    metadata.json extension.js prefs.js stylesheet.css tokens.json \
    lib schemas )
echo "    -> $HERE/$UUID.zip"

if [[ "${1:-}" == "install" ]]; then
    DEST="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions/$UUID"
    echo "==> Installing to $DEST"
    rm -rf "$DEST"
    mkdir -p "$DEST"
    ( cd "$HERE" && cp -r metadata.json extension.js prefs.js stylesheet.css tokens.json lib schemas "$DEST/" )
    echo "    Done. Log out/in (Xorg: Alt+F2 'r') and enable with:"
    echo "      gnome-extensions enable $UUID"
fi
