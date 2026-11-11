#!/usr/bin/env bash
# Measure frame times. Prints PERF lines (JSON) with fps and frame-time percentiles.
#   tools/perf.sh            GPU cost at full-screen size with no window, at three zooms, then a
#                            short windowed run (a small, unfocused window for about 7 seconds)
#   tools/perf.sh --no-window   skip the windowed run
# Extra flags after those go to the game (e.g. --seed 3 --day 14).
set -euo pipefail
cd "$(dirname "$0")/.."
window=1
if [ "${1:-}" = "--no-window" ]; then window=0; shift; fi
cargo build --release -q -p colony
for z in 16 40 120; do
  echo -n "zoom $z: "
  ./target/release/colony --bench 3024x1964 --frames 300 --zoom "$z" "$@"
done
if [ "$window" = 1 ]; then
  ./target/release/colony --perf 5 --background --window 1280x800 "$@" | tail -2
fi
./target/release/colony --pan-test "$@"
