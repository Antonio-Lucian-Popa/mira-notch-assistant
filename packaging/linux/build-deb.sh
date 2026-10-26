#!/usr/bin/env bash
# Build a .deb that installs the Mira daemon, its systemd user unit + D-Bus
# activation file, and the GNOME Shell extension. extensions.gnome.org cannot
# host the daemon binary, so the .deb is how Mira ships on Ubuntu.
#
#   ./build-deb.sh            build target/debian/mira_<ver>_amd64.deb
#
# Requires: cargo, dpkg-deb, glib-compile-schemas, zip (via build.sh).

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$HERE/../.." && pwd)"
UUID="mira@novabytecode.ro"
# Version: $MIRA_VERSION wins (set by CI to match the release tag), else the
# workspace version from Cargo.toml.
VERSION="${MIRA_VERSION:-$(grep -m1 '^version' "$REPO_ROOT/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')}"
ARCH="amd64"
PKG="mira_${VERSION}_${ARCH}"
BUILD="$HERE/build/$PKG"
OUT="$REPO_ROOT/target/debian"

echo "==> Building mira-daemon (release)"
cargo build --release -p mira-daemon --manifest-path "$REPO_ROOT/Cargo.toml"

echo "==> Preparing extension payload"
bash "$REPO_ROOT/gnome-extension/build.sh" >/dev/null

echo "==> Assembling package tree at $BUILD"
rm -rf "$HERE/build"
mkdir -p \
    "$BUILD/DEBIAN" \
    "$BUILD/usr/bin" \
    "$BUILD/usr/lib/systemd/user" \
    "$BUILD/usr/share/dbus-1/services" \
    "$BUILD/usr/share/gnome-shell/extensions/$UUID"

install -m 0755 "$REPO_ROOT/target/release/mira-daemon" "$BUILD/usr/bin/mira-daemon"
install -m 0755 "$HERE/mira-add" "$BUILD/usr/bin/mira-add"
install -m 0644 "$HERE/systemd/mira-daemon.service" "$BUILD/usr/lib/systemd/user/mira-daemon.service"
install -m 0644 "$HERE/dbus/ro.novabytecode.Mira.service" "$BUILD/usr/share/dbus-1/services/ro.novabytecode.Mira.service"

EXT_SRC="$REPO_ROOT/gnome-extension"
EXT_DST="$BUILD/usr/share/gnome-shell/extensions/$UUID"
cp -r "$EXT_SRC/metadata.json" "$EXT_SRC/extension.js" "$EXT_SRC/prefs.js" \
      "$EXT_SRC/stylesheet.css" "$EXT_SRC/tokens.json" \
      "$EXT_SRC/lib" "$EXT_SRC/schemas" "$EXT_DST/"

SIZE_KB="$(du -sk "$BUILD" | cut -f1)"

cat > "$BUILD/DEBIAN/control" <<EOF
Package: mira
Version: $VERSION
Section: gnome
Priority: optional
Architecture: $ARCH
Depends: gnome-shell (>= 46), dbus
Installed-Size: $SIZE_KB
Maintainer: Mira contributors <noreply@novabytecode.ro>
Homepage: https://github.com/antonio-lucian-popa/mira-notch-assistant
Description: Mira dynamic-island notch for GNOME
 A lightweight, local-first notch for the top of the screen showing music,
 synced lyrics, clock and system alerts. Installs the Mira daemon (a D-Bus
 service), its systemd user unit, and the GNOME Shell extension UI.
EOF

cat > "$BUILD/DEBIAN/postinst" <<EOF
#!/bin/sh
set -e
# Compile the extension's GSettings schema in place.
if command -v glib-compile-schemas >/dev/null 2>&1; then
    glib-compile-schemas "/usr/share/gnome-shell/extensions/$UUID/schemas" || true
fi
echo "Mira installed. Enable it per-user with:"
echo "  systemctl --user enable --now mira-daemon.service"
echo "  gnome-extensions enable $UUID   # after logging out and back in"
exit 0
EOF
chmod 0755 "$BUILD/DEBIAN/postinst"

cat > "$BUILD/DEBIAN/prerm" <<EOF
#!/bin/sh
set -e
# Best-effort stop of the per-user service for the invoking user.
if [ -n "\${SUDO_USER:-}" ] && command -v systemctl >/dev/null 2>&1; then
    su - "\$SUDO_USER" -c "systemctl --user stop mira-daemon.service" 2>/dev/null || true
fi
exit 0
EOF
chmod 0755 "$BUILD/DEBIAN/prerm"

echo "==> Building .deb"
mkdir -p "$OUT"
dpkg-deb --root-owner-group --build "$BUILD" "$OUT/$PKG.deb"
echo "    -> $OUT/$PKG.deb"
