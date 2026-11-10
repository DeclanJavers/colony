#!/usr/bin/env bash
# Run the game for an automated screenshot on the display the user is NOT using (screen 0), in the
# background: macOS `open -g` launches it without activating it, so it never steals focus from the
# window you're working in. Usage: tools/shot.sh out.png [extra game args...]
#   e.g. tools/shot.sh /tmp/a.png --prewarm 3 --zoom 14 --select
# SCREEN=1 tools/shot.sh ... for the other display (screen 1 is the one the user works on).
# GODOT_ARGS="--resolution 3024x1890" tools/shot.sh ... for a window of another size.
set -euo pipefail
out="$1"; shift
cd "$(dirname "$0")/.."
root="$PWD"
log="$(mktemp -t colony_shot)"
# -W wait for it to quit, -g don't bring it to the front, -n always a fresh instance.
perl -e 'alarm 120; exec @ARGV' open -W -g -n -a /Applications/Godot.app --stdout "$log" --stderr "$log" \
  --args --path "$root" --screen "${SCREEN:-0}" ${GODOT_ARGS:-} -- "$@" --shot "$out" --shot-delay "${DELAY:-5}" || {
  pkill -f -- "--shot $out" 2>/dev/null || true
  echo "shot.sh: timed out" >&2
}
cat "$log"
rm -f "$log"
