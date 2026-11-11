"""Smoke test for the brain server. Replays real colonist states from a decision log
through /decide and compares Laya's picks with what UtilityBrain chose.

  brain_server/.venv/bin/python tests/smoke_brain.py [logs/decisions/<run>.jsonl] [N]
"""
import collections
import glob
import json
import os
import sys
import time

import httpx

URL = os.environ.get("BRAIN_URL", "http://127.0.0.1:8765")
path = sys.argv[1] if len(sys.argv) > 1 else max(glob.glob("logs/decisions/*.jsonl"), key=os.path.getmtime)
n = int(sys.argv[2]) if len(sys.argv) > 2 else 30

rows = [json.loads(l) for l in open(path)]
step = max(1, len(rows) // n)
sample = rows[::step][:n]

print("health:", httpx.get(URL + "/health", timeout=5).json())

reqs = [{"request_id": i, "colonist_name": r["name"], "state_text": r["state_text"],
         "options": r["options"], "build_options": r["build_options"], "social_options": r["social_options"]}
        for i, r in enumerate(sample)]
t = time.time()
res = httpx.post(URL + "/decide", json={"requests": reqs}, timeout=300).json()["results"]
dt = time.time() - t

agree = 0
picks = collections.Counter()
for r, d in zip(sample, res):
    if d.get("error"):
        print("ERROR", d["error"])
        continue
    picks[d["action"]] += 1
    agree += d["action"] == r["chosen"]
print(f"\n{len(res)} decisions in {dt:.1f}s ({dt / len(res) * 1000:.0f} ms each)")
print(f"agreement with UtilityBrain: {agree}/{len(res)}")
print("Laya's picks:", dict(picks))

ex = sample[len(sample) // 2]
d = res[len(sample) // 2]
print("\nExample state:\n ", ex["state_text"])
print("  utility chose:", ex["chosen"], "| laya chose:", d["action"])
print("  laya probs:", {k: round(v, 3) for k, v in sorted(d["probs"].items(), key=lambda kv: -kv[1])})
if d.get("build_probs"):
    print("  build probs:", {k: round(v, 3) for k, v in d["build_probs"].items()})

if "--chat" in sys.argv:
    col = {"name": ex["name"], "stage": ex["stage"], "traits": [], "need_words": ["a bit hungry"],
           "action": ex["chosen"], "colony": "5 colonists, a stockpile and a campfire", "memories": []}
    t = time.time()
    c = httpx.post(URL + "/chat", json={"colonist": col, "history": [], "message": "What are you doing, and why?"}, timeout=120).json()
    print(f"\nchat ({(time.time() - t):.1f}s):", c)
    m = httpx.post(URL + "/remember", json={"colonist": col, "transcript": [
        {"role": "user", "content": "You should build a well soon, the river is far."},
        {"role": "assistant", "content": c.get("reply", "")}]}, timeout=120).json()
    print("memory:", m)
