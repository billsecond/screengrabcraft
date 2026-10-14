#!/usr/bin/env bash
# Build and package ScreenGrabCraft for FreeBSD:
#
#   $DIST/screengrabcraft-<version>-freebsd-<arch>.tar.gz
#       bin/    screengrabcraft (capture service), screengrabcraft-editor, screengrabcraft-cli
#       share/  desktop entries, metainfo, MIME type, icons, docs, and the capture service's
#               autostart entry (share/screengrabcraft/autostart/, to copy into
#               /usr/local/etc/xdg/autostart or ~/.config/autostart)
#
# Usage: packaging/freebsd/package.sh [--skip-build]
#
# Needs: bash, cargo, and the libraries the windows load at run time (libxkbcommon, wayland or
# libX11, mesa-libs or vulkan-loader). Run by .github/workflows/package.yml inside a FreeBSD VM.
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
LINUX="$ROOT/packaging/linux"
APP_ID=io.github.billsecond.screengrabcraft
EDITOR_ID=io.github.billsecond.screengrabcraft.editor

SKIP_BUILD=0
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    -h | --help) sed -n '2,13p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

ARCH="$(uname -m)"
case "$ARCH" in
  amd64 | x86_64) ARCH=x86_64 ;;
  arm64 | aarch64) ARCH=aarch64 ;;
  *) echo "unsupported architecture $ARCH" >&2; exit 2 ;;
esac
BASENAME="screengrabcraft-$VERSION-freebsd-$ARCH"

echo "==> ScreenGrabCraft $VERSION for FreeBSD $ARCH"

if [ "$SKIP_BUILD" = 0 ]; then
  (cd "$ROOT" && cargo build --release --locked -p screengrabcraft -p screengrabcraft-editor -p screengrabcraft-cli)
fi
BIN="$CARGO_TARGET_DIR/release"
WORK="$CARGO_TARGET_DIR/freebsd-package"
STAGE="$WORK/$BASENAME"
rm -rf "$WORK"

# BSD install has no -D: make the directories first.
mkdir -p "$STAGE/bin" "$STAGE/share/applications" "$STAGE/share/mime/packages" "$STAGE/share/metainfo" \
  "$STAGE/share/icons" "$STAGE/share/doc/screengrabcraft" "$STAGE/share/screengrabcraft/autostart"
for bin in screengrabcraft screengrabcraft-editor screengrabcraft-cli; do
  install -m 755 "$BIN/$bin" "$STAGE/bin/$bin"
  strip "$STAGE/bin/$bin" 2>/dev/null || true
done
for id in "$APP_ID" "$EDITOR_ID"; do
  install -m 644 "$LINUX/$id.desktop" "$STAGE/share/applications/$id.desktop"
  sed -e "s/@VERSION@/$VERSION/g" -e "s/@DATE@/$SCREENGRABCRAFT_BUILD_DATE/g" \
    "$LINUX/$id.metainfo.xml.in" >"$STAGE/share/metainfo/$id.metainfo.xml"
done
install -m 644 "$LINUX/$APP_ID.mime.xml" "$STAGE/share/mime/packages/$APP_ID.xml"
install -m 644 "$LINUX/autostart/$APP_ID.desktop" "$STAGE/share/screengrabcraft/autostart/$APP_ID.desktop"
cp -R "$ROOT/assets/app-icon/hicolor" "$STAGE/share/icons/"
copy_docs "$STAGE/share/doc/screengrabcraft"

tar -C "$WORK" -czf "$DIST/$BASENAME.tar.gz" "$BASENAME"
echo "wrote $DIST/$BASENAME.tar.gz"
for bin in screengrabcraft screengrabcraft-editor screengrabcraft-cli; do
  "$STAGE/bin/$bin" --version
done
echo "==> done"
