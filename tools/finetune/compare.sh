#!/usr/bin/env bash
# Compare brains on the same colonies. Runs headless (no windows), one colony at a time.
#   tools/finetune/compare.sh [days] [seeds...]
# Needs the brain server running for the Laya rows (brain_server/run.sh serves the newest model).
# Prints one line per brain and seed: survivors, deaths, births, tier, failed decisions, and how
# often colonists talked (asks / news), checked the stores, or did what they were asked.
set -uo pipefail
cd "$(dirname "$0")/../.."
days="${1:-6}"; shift || true
seeds=("${@:-1234 77 4242}")
G=/Applications/Godot.app/Contents/MacOS/Godot
for seed in ${seeds[@]}; do
  for brain in utility laya; do
    out=$(perl -e 'alarm 1800; exec @ARGV' nice -n 5 $G --headless --path . -s res://tests/run_headless.gd -- \
      --days "$days" --seed "$seed" --brain "$brain" --log 1 --watcher-requests 6 2>&1)
    final=$(echo "$out" | grep '^FINAL' | sed 's/^FINAL //')
    choices=$(echo "$out" | grep '^CHOICES' | sed 's/^CHOICES //')
    failed=$(echo "$out" | grep -o 'failed decisions: [0-9]* of [0-9]*')
    python3 - "$brain" "$seed" "$final" "$choices" "$failed" <<'EOF'
import json, sys
brain, seed, final, choices, failed = sys.argv[1:6]
f = json.loads(final or "{}"); c = json.loads(choices or "{}")
print(f"{brain:8s} seed {seed:5s} pop {f.get('pop','?'):>3} deaths {sum(f.get('deaths',{}).values()):>2} "
      f"births {f.get('births','?'):>2} tier {f.get('tier','?')}  {failed:28s} "
      f"ask {c.get('ask_help',0):>3} news {c.get('share_news',0):>3} check {c.get('check_stores',0):>3}")
EOF
  done
done
