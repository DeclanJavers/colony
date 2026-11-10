#!/usr/bin/env bash
# Measure the game's frame rate on screen 0 without taking focus (see main.gd --perf).
# Usage: tools/perf.sh [seconds] [extra game args...]   e.g. tools/perf.sh 8 --prewarm 5 --zoom 20
# The window closes itself when done; if it hangs, it's killed after PERF_TIMEOUT (90) s.
set -euo pipefail
secs="${1:-8}"; shift || true
cd "$(dirname "$0")/.."
log="$(mktemp -t colony_perf)"
tag="perf$$"
perl -e "alarm ${PERF_TIMEOUT:-90}; exec @ARGV" open -W -g -n -a /Applications/Godot.app --stdout "$log" --stderr "$log" \
  --args --path "$PWD" --screen "${SCREEN:-0}" -- "$@" --perf "$secs" --perf-tag "$tag" || {
  pkill -f -- "--perf-tag $tag" 2>/dev/null || true
  echo "perf.sh: timed out" >&2
}
grep -E "^PERF|^PROBE|^SLOW|SCRIPT ERROR" "$log" || tail -5 "$log"
rm -f "$log"
