"""Compare language models on the held-out jobs (M11 done-check): base LFM3 against the fine-tuned
one, through llama-server, the way the game asks them.

    python3 brain/lm/eval_local.py brain/data/lm-v1/lm.jsonl.gz --port 8766 [--n 40] [--label base]

For each task: how many answers are valid (the same checks the game makes: JSON of the right
shape, an option that exists, lines by the two people talking, a sentence of sane length) and,
for decisions, how often the model picks what Haiku picked. Writes runs/lm-eval/<label>.json.
"""

import gzip
import json
import os
import re
import sys
import time
import urllib.request

path = sys.argv[1]
arg = lambda k, d: sys.argv[sys.argv.index(k) + 1] if k in sys.argv else d
port = int(arg("--port", "8766"))
n_per = int(arg("--n", "40"))
label = arg("--label", f"port-{port}")

rows = [json.loads(l) for l in gzip.open(path, "rt")]
held = [r for r in rows if r["split"] != "train"]


def chat(system, user):
    body = json.dumps({
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
        "max_tokens": 220, "temperature": 0.4, "top_p": 0.9,
        "response_format": {"type": "json_object"},
        "chat_template_kwargs": {"enable_thinking": False},
    }).encode()
    req = urllib.request.Request(f"http://127.0.0.1:{port}/v1/chat/completions", body, {"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=120) as r:
        return json.loads(r.read())["choices"][0]["message"]["content"]


def first_json(s):
    try:
        return json.loads(s)
    except ValueError:
        a, b = s.find("{"), s.rfind("}")
        try:
            return json.loads(s[a:b + 1]) if a >= 0 and b > a else None
        except ValueError:
            return None


def sentence(v, max_words):
    return isinstance(v, str) and 1 <= len(v.split()) <= max_words and len(v) <= max_words * 12


def options_in(user):
    tail = user.split("What do you do next?")[-1] if "What do you do next?" in user else user.split("What goal do you set")[-1]
    return len([l for l in tail.splitlines() if re.match(r"^\d+\. ", l.strip())])


def names_in(user):
    lines = user.splitlines()
    return [l.split(",")[0].strip() for l in lines[1:3]]


def valid(task, user, j):
    if not isinstance(j, dict):
        return False
    if task in ("step", "goal"):
        c = j.get("choice")
        return isinstance(c, int) and 1 <= c <= options_in(user) and (j.get("why") is None or sentence(j.get("why"), 40))
    if task == "talk":
        names = [n.lower() for n in names_in(user)] + ["a", "b"]
        ls = j.get("lines")
        return isinstance(ls, list) and 2 <= len(ls) <= 5 and all(isinstance(l, dict) and str(l.get("by", "")).strip().lower() in names and sentence(l.get("says"), 24) for l in ls)
    if task == "memory":
        return sentence(j.get("words"), 40)
    if task == "words":
        return isinstance(j.get("acts", []), list) and isinstance(j.get("believe"), bool) and (j.get("reply") is None or sentence(j.get("reply"), 20))
    return False


report = {"label": label, "port": port, "tasks": {}}
for task in ["goal", "step", "talk", "memory", "words"]:
    rs = [r for r in held if r["task"] == task][:n_per]
    if not rs:
        continue
    ok = agree = 0
    ms = 0.0
    samples = []
    for r in rs:
        t0 = time.time()
        try:
            got = chat(r["system"], r["user"])
        except Exception as e:  # noqa: BLE001
            got = f"ERROR {e}"
        ms += (time.time() - t0) * 1000
        j = first_json(got)
        good = valid(task, r["user"], j)
        ok += good
        if good and task in ("step", "goal"):
            agree += j["choice"] == json.loads(r["assistant"])["choice"]
        if task == "words" and good:
            h = json.loads(r["assistant"])
            agree += bool(j.get("believe")) == bool(h.get("believe")) and sorted(j.get("acts", [])) == sorted(h.get("acts", []))
        if len(samples) < 3:
            samples.append({"haiku": r["assistant"], "model": got})
    n = len(rs)
    report["tasks"][task] = {"n": n, "valid": ok / n, "agree_with_haiku": agree / n if task in ("step", "goal", "words") else None, "ms": ms / n, "samples": samples}
    a = f", same as Haiku {agree / n:.0%}" if task in ("step", "goal", "words") else ""
    print(f"{label} {task}: {ok}/{n} valid ({ok / n:.0%}){a}, {ms / n:.0f} ms", flush=True)

os.makedirs("runs/lm-eval", exist_ok=True)
json.dump(report, open(f"runs/lm-eval/{label}.json", "w"), indent=1)
print(f"wrote runs/lm-eval/{label}.json")
