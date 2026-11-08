"""Evaluate brains on the same colonies and print a scoreboard (also saved to logs/eval/).

  python3 tools/eval/run_eval.py --brains utility laya --seeds 1234 77 4242 --days 8 [--label v2]

Each run is headless (no windows), one at a time. 'laya' uses whatever model the brain server is
serving, so evaluate a new model by restarting the server on it and passing --label. Laya runs
also log the teacher's answer for every state, ready for retraining on Laya's own mistakes.
Metrics come from scripts/sim/colony_metrics.gd; flags mark things that look unhealthy.
"""
import argparse
import datetime
import json
import os
import subprocess
import sys

GODOT = "/Applications/Godot.app/Contents/MacOS/Godot"
ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

ap = argparse.ArgumentParser()
ap.add_argument("--brains", nargs="+", default=["utility", "laya"])
ap.add_argument("--seeds", nargs="+", type=int, default=[1234, 77, 4242])
ap.add_argument("--days", type=int, default=8)
ap.add_argument("--label", default="", help="name for the Laya model being evaluated, e.g. v2")
ap.add_argument("--watcher-requests", type=int, default=6)
a = ap.parse_args()

rows = []
for seed in a.seeds:
    for brain in a.brains:
        cmd = ["perl", "-e", "alarm 3600; exec @ARGV", "nice", "-n", "5", GODOT, "--headless", "--path", ROOT,
               "-s", "res://tests/run_headless.gd", "--", "--days", str(a.days), "--seed", str(seed),
               "--brain", brain, "--log", "1", "--watcher-requests", str(a.watcher_requests)]
        out = subprocess.run(cmd, capture_output=True, text=True).stdout
        m = next((l[8:] for l in out.splitlines() if l.startswith("METRICS ")), None)
        log = next((l.split("decision log: ")[1].split(" (")[0] for l in out.splitlines() if "decision log: " in l), "")
        if not m:
            print(f"{brain} seed {seed}: no metrics (crashed?)", file=sys.stderr)
            continue
        r = json.loads(m)
        name = brain if brain != "laya" or not a.label else f"laya {a.label}"
        rows.append({"brain": name, "seed": seed, "log": log, **r})
        print(f"{name:10s} seed {seed:5d}: pop {r['pop']:2d} deaths {sum(r['deaths'].values())} "
              f"homeless {r['homeless_share']:.2f} farms/person {r['max_farms_per_person']:.1f} "
              f"failed {r['failed_share']:.3f} flags {r['flags']}", flush=True)

# Scoreboard: averages per brain.
def avg(rs, k):
    return sum(float(x[k]) for x in rs) / max(1, len(rs))

brains = sorted({r["brain"] for r in rows}, key=lambda b: [r["brain"] for r in rows].index(b))
cols = ["pop", "births", "tier4_day", "homeless_share", "max_farms_per_person", "famine_share",
        "critical_share", "failed_share", "belief_error"]
lines = [f"# Brain evaluation {datetime.datetime.now():%Y-%m-%d %H:%M}", "",
         f"{a.days} days x seeds {a.seeds}, Watcher requests every {a.watcher_requests}h", "",
         "| brain | " + " | ".join(cols) + " | deaths | talks | requests followed | flags |",
         "|" + "---|" * (len(cols) + 5)]
for b in brains:
    rs = [r for r in rows if r["brain"] == b]
    deaths = sum(sum(r["deaths"].values()) for r in rs)
    talks = sum(sum(r["talks"].values()) for r in rs)
    made = sum(r["requests_made"] for r in rs)
    followed = sum(r["requests_followed"] for r in rs)
    flags = sorted({f for r in rs for f in r["flags"]})
    lines.append(f"| {b} | " + " | ".join(f"{avg(rs, c):.2f}" for c in cols) +
                 f" | {deaths} | {talks} | {followed}/{made} | {'; '.join(flags) or '-'} |")
report = "\n".join(lines) + "\n"
print("\n" + report)
os.makedirs(os.path.join(ROOT, "logs", "eval"), exist_ok=True)
stamp = datetime.datetime.now().strftime("%Y-%m-%d_%H-%M")
with open(os.path.join(ROOT, "logs", "eval", f"{stamp}{'_' + a.label if a.label else ''}.md"), "w") as f:
    f.write(report)
with open(os.path.join(ROOT, "logs", "eval", f"{stamp}{'_' + a.label if a.label else ''}.json"), "w") as f:
    json.dump(rows, f, indent=1)
