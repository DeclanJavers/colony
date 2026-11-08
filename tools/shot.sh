#!/usr/bin/env bash
# Run the game for an automated screenshot on the display the user is NOT using (screen 0), so test windows don't cover
# whatever you're working on. Usage: tools/shot.sh out.png [extra game args...]
#   e.g. tools/shot.sh /tmp/a.png --prewarm 3 --zoom 14 --select
# SCREEN=1 tools/shot.sh ... for the other display (screen 1 is the one the user works on).
set -euo pipefail
out="$1"; shift
cd "$(dirname "$0")/.."
exec perl -e 'alarm 90; exec @ARGV' /Applications/Godot.app/Contents/MacOS/Godot --path . \
  --screen "${SCREEN:-0}" -- "$@" --shot "$out" --shot-delay "${DELAY:-5}"
