"""Build the language model's fine-tuning set (M11): Haiku's answers to every language job, as chat
examples (system, user, assistant) in the format brain/lm/train_lora.py reads.

    python3 brain/lm/build_dataset.py runs/lm-label-1/dataset.jsonl brain/data/haiku-v4/text.jsonl.gz brain/data/lm-v1

The first file is `colony-run label --task goal,talk,memory,words`'s dataset (already checked
the way the game checks the model's answers). The second is Haiku's step decisions (M9): a
sample of them becomes step examples. Their instructions come from the game itself
(`colony-run lm --print-system step`), so they're exactly what LFM3 will be sent.
"""

import gzip
import json
import os
import random
import subprocess
import sys

random.seed(0)
lm_rows, steps_path, out_dir = sys.argv[1], sys.argv[2], sys.argv[3]
STEPS = int(os.environ.get("STEPS", "1500"))

# The step instructions, exactly as the game sends them (from the Rust source of truth).
step_system = subprocess.run(["./target/release/colony-run", "lm", "--print-system", "step"], capture_output=True, text=True, check=True).stdout.rstrip("\n")

rows = [json.loads(l) for l in open(lm_rows) if l.strip()]
steps = [json.loads(l) for l in gzip.open(steps_path, "rt") if l.strip()]
random.shuffle(steps)
added = 0
for s in steps:
    if added >= STEPS:
        break
    if not s.get("reason") or not s.get("choice"):
        continue
    rows.append({
        "task": "step",
        "split": s["split"] if s["split"] == "train" else "held",
        "seed": s["seed"],
        "tick": s.get("tick", 0),
        "system": step_system,
        "user": s["prompt"],
        "assistant": json.dumps({"choice": s["choice"], "why": s["reason"]}, separators=(",", ":")),
        "code": str(s.get("rules_choice")),
    })
    added += 1

os.makedirs(out_dir, exist_ok=True)
with gzip.open(os.path.join(out_dir, "lm.jsonl.gz"), "wt") as f:
    for r in rows:
        f.write(json.dumps(r) + "\n")
count = {}
for r in rows:
    k = (r["task"], r["split"])
    count[k] = count.get(k, 0) + 1
print(f"{len(rows)} examples:", ", ".join(f"{t} {s} {n}" for (t, s), n in sorted(count.items())))
