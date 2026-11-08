"""Does the brain act on requests? For held-out states that contain a request, compare the
probability of the requested action with and without the "Requests: ..." sentence (and the matching
"asked me to" memories), using whatever model the brain server is running.

  python3 tools/finetune/request_test.py logs/finetune/data_v2/eval.jsonl.gz [N]
"""
import gzip
import json
import re
import sys
import urllib.request

path = sys.argv[1]
n_max = int(sys.argv[2]) if len(sys.argv) > 2 else 80
URL = "http://127.0.0.1:8765/decide"
REQ_RE = re.compile(r" Requests: [^.]*\.")
MEM_RE = re.compile(r"Day \d+: [^.]* asked me to [^.]*\. ?")


def decide(state: str, row: dict) -> dict:
    q = row["questions"]["action"]
    opts = [{"id": lbl, "label": lbl, "description": desc} for lbl, desc in q["criteria"].items()]
    name = q["instructions"].replace("What should ", "").replace(" do next?", "")
    ctx = {"request_id": 1, "colonist_id": 0, "colonist_name": name, "state_text": state, "options": opts,
           "build_options": [], "social_options": []}
    req = urllib.request.Request(URL, json.dumps({"requests": [ctx]}).encode(), {"Content-Type": "application/json"})
    return json.load(urllib.request.urlopen(req))["results"][0]["probs"]


rows = []
for line in gzip.open(path, "rt"):
    r = json.loads(line)
    m = re.search(r"Requests: .*? asked \w+ to ([a-z ]+) \(", r["state"])
    if m and m.group(1) in r["questions"]["action"]["criteria"]:
        r["_asked"] = m.group(1)
        rows.append(r)
    if len(rows) >= n_max:
        break

with_p, without_p, top_with, top_without = [], [], 0, 0
for r in rows:
    asked = r["_asked"]
    stripped = MEM_RE.sub("", REQ_RE.sub("", r["state"]))
    pw, po = decide(r["state"], r), decide(stripped, r)
    with_p.append(pw.get(asked, 0.0))
    without_p.append(po.get(asked, 0.0))
    top_with += max(pw, key=pw.get) == asked
    top_without += max(po, key=po.get) == asked
n = len(rows)
print(f"{n} states with a request")
print(f"P(requested action): with request {sum(with_p) / n:.3f}  without {sum(without_p) / n:.3f}")
print(f"requested action is the top choice: with {top_with}/{n}  without {top_without}/{n}")
