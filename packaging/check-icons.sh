#!/usr/bin/env bash
# Check that every app icon file the packages use is there, and that each PNG is the size its
# name or folder says. Run by packaging-lint (storytold/craft-common-actions) when present; also
# by packaging/icons.sh after drawing them.
#
#   packaging/check-icons.sh
#
# Both programs have the same set: the capture service (screengrabcraft.*,
# io.github.billsecond.screengrabcraft.png) and the editor (screengrabcraft-editor.*,
# io.github.billsecond.screengrabcraft.editor.png).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ICONS="$ROOT/assets/app-icon"

failed=0
fail() { echo "error: $*" >&2; failed=1; }

# Width and height of a PNG from its IHDR chunk ("W H"); nothing when the file isn't a PNG.
png_size() {
  [ "$(od -An -tx1 -N8 "$1" | tr -d ' \n')" = 89504e470d0a1a0a ] || return 0
  od -An -tu1 -j16 -N8 "$1" | awk '{ print $1 * 16777216 + $2 * 65536 + $3 * 256 + $4, $5 * 16777216 + $6 * 65536 + $7 * 256 + $8 }'
}

# check_file <path>; check_png <path> <size>
check_file() { [ -s "$1" ] || fail "missing ${1#"$ROOT"/}"; }
check_png() {
  local got
  if [ ! -s "$1" ]; then fail "missing ${1#"$ROOT"/}"; return; fi
  got="$(png_size "$1")"
  [ "$got" = "$2 $2" ] || fail "${1#"$ROOT"/} is ${got:-not a PNG}, expected $2 x $2"
}

for pair in screengrabcraft:io.github.billsecond.screengrabcraft \
  screengrabcraft-editor:io.github.billsecond.screengrabcraft.editor; do
  name="${pair%%:*}"
  id="${pair#*:}"
  # Windows (MSI, shortcuts, the programs' resources) and macOS (the app bundles).
  check_file "$ICONS/$name.ico"
  check_file "$ICONS/$name.icns"
  check_png "$ICONS/$name-1024.png" 1024
  check_png "$ICONS/$name-macos-512.png" 512
  # Linux and FreeBSD (hicolor theme), the Flatpak and the AppImage.
  for size in 16 24 32 48 64 128 256 512; do
    check_png "$ICONS/hicolor/${size}x${size}/apps/$id.png" "$size"
  done
done

if [ "$failed" != 0 ]; then
  echo "app icons incomplete: run packaging/icons.sh (cargo xtask icon) and commit the result" >&2
  exit 1
fi
echo "app icons ok"
