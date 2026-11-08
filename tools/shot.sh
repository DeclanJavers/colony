#!/usr/bin/env bash
# Run the game for an automated screenshot on the second display, so test windows don't cover
# whatever you're working on. Usage: tools/shot.sh out.png [extra game args...]
#   e.g. tools/shot.sh /tmp/a.png --prewarm 3 --zoom 14 --select
# SCREEN=0 tools/shot.sh ... to use the main display instead.
set -euo pipefail
out="$1"; shift
cd "$(dirname "$0")/.."
exec perl -e 'alarm 90; exec @ARGV' /Applications/Godot.app/Contents/MacOS/Godot --path . \
  --screen "${SCREEN:-1}" -- "$@" --shot "$out" --shot-delay "${DELAY:-5}"
