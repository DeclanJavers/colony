#!/usr/bin/env bash
# Render one frame of the game with no window and save it (nothing opens on screen).
#   tools/shot.sh out.png [colony flags...]
# Examples:
#   tools/shot.sh runs/shots/day.png --seed 2 --day 8 --time 11
#   tools/shot.sh runs/shots/night.png --day 8 --time 22 --zoom 9
#   tools/shot.sh runs/shots/winter.png --day 20 --time 11
#   tools/shot.sh runs/shots/close.png --zoom 7 --x 191.5 --z 143.5    (find spots: colony-run find)
# Flags: --seed --size --day --time --zoom --x --z --shot-size WxH --readout
set -euo pipefail
cd "$(dirname "$0")/.."
out="${1:?usage: tools/shot.sh out.png [flags...]}"
shift
cargo build --release -q -p colony
./target/release/colony --shot "$out" "$@"
