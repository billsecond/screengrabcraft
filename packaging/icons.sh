#!/usr/bin/env bash
# Regenerate both app icons (the capture service's and the editor's): the PNGs, the Windows .ico,
# the macOS .icns and the Linux hicolor tree, in assets/app-icon/.
#
# The artwork is drawn in code (xtask), so this needs nothing but cargo. The outputs are
# committed, so builds and packaging never need to run it. Change the drawing, run this, check
# the result with packaging/check-icons.sh, and commit what changed (every file is listed in
# ATTRIBUTION.md).
#
#   packaging/icons.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
cargo run -q -p xtask -- icon
bash packaging/check-icons.sh
