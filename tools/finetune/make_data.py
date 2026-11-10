"""Turn decision logs into a Laya fine-tuning dataset ({state, questions, gold} JSONL).

Each logged decision becomes one row: the colonist's state text, the exact questions the brain
server asks (brain_server/laya_format.py), and the teacher's probabilities as the soft target.
By default the teacher is the hand-made UtilityBrain, so this distils it into Laya; later the
same script can train on labelled or outcome-filtered logs.

  python tools/finetune/make_data.py --out data_dir --eval-seeds 901,902 logs/decisions/*utility*.jsonl*
  (old logs are gzipped: .jsonl.gz is read the same way)
"""

from __future__ import annotations

import argparse
import collections
import gzip
import json
import os
import random
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", "brain_server"))
from laya_format import OPTION_FIELDS, PROB_FIELDS, QUESTIONS_VERSION, build_questions, id_to_label  # noqa: E402


def to_row(r: dict, use_teacher: bool = False) -> dict | None:
    if use_teacher:
        t = r.get("teacher")
        if not t or not t.get("probs"):
            return None
        # Relabel: the state the learner reached, the teacher's answer as the target.
        r = {**r, "probs": t["probs"], "build_probs": t.get("build_probs", {}),
             "social_probs": t.get("social_probs", {}), "chosen": t.get("chosen", "")}
    if not r.get("options") or not r.get("probs"):
        return None
    questions = build_questions(r)
    gold = {}
    for qid in questions:
        opts, probs = r.get(OPTION_FIELDS[qid], []), r.get(PROB_FIELDS[qid], {})
        labels = id_to_label(opts)
        p = {labels[str(k)]: float(v) for k, v in probs.items() if str(k) in labels}
        if p and sum(p.values()) > 0:
            gold[qid] = {"probabilities": p}
    if "action" not in gold:
        return None
    return {"state": r["state_text"], "questions": {q: questions[q] for q in gold}, "gold": gold,
            "meta": {"seed": r.get("seed"), "chosen": r.get("chosen"), "brain": r.get("brain"),
                     "describer": r.get("describer_version"), "questions_version": QUESTIONS_VERSION}}


def key(r: dict) -> tuple:
    ids = lambda f: tuple(str(o["id"]) for o in r.get(f, []))
    return (r["state_text"], ids("options"), ids("build_options"), ids("social_options"))


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("logs", nargs="+")
    ap.add_argument("--out", required=True)
    ap.add_argument("--eval-seeds", default="")
    ap.add_argument("--brain", default="utility", help="only use decisions made by this brain")
    ap.add_argument("--max-train", type=int, default=0, help="subsample train rows (0 = all)")
    ap.add_argument("--cap-per-choice", type=int, default=0,
                    help="keep at most this many train rows per target choice (0 = no cap): trims the very "
                         "common ones (rest, socialize) so rare but important ones (explore, meetings) aren't drowned")
    ap.add_argument("--mistakes", type=float, default=-1.0,
                    help="with --teacher: keep every state where the learner's choice differed from the "
                         "teacher's, plus this fraction of the ones where they agreed (e.g. 0.25)")
    ap.add_argument("--teacher", action="store_true",
                    help="use the logged teacher's answer as the target (relabel a learner's own states); "
                         "with this, --brain selects whose states (e.g. laya)")
    a = ap.parse_args()
    eval_seeds = {int(s) for s in a.eval_seeds.split(",") if s}

    random.seed(0)
    train, evals, seen = [], [], set()
    for path in a.logs:
        for line in (gzip.open(path, "rt") if path.endswith(".gz") else open(path)):
            r = json.loads(line)
            if r.get("brain") != a.brain:
                continue
            k = key(r)
            if k in seen:
                continue
            if a.teacher and a.mistakes >= 0.0 and r.get("teacher"):
                agreed = r.get("chosen") == r["teacher"].get("chosen")
                if agreed and random.random() > a.mistakes:
                    continue
            seen.add(k)
            row = to_row(r, a.teacher)
            if row:
                (evals if r.get("seed") in eval_seeds else train).append(row)
    random.seed(0)
    random.shuffle(train)
    if a.cap_per_choice:
        per = collections.Counter()
        capped = []
        for row in train:
            c = row["meta"]["chosen"]
            if per[c] < a.cap_per_choice:
                per[c] += 1
                capped.append(row)
        train = capped
    if a.max_train:
        train = train[: a.max_train]
    os.makedirs(a.out, exist_ok=True)
    for name, rows in (("train", train), ("eval", evals)):
        with gzip.open(os.path.join(a.out, f"{name}.jsonl.gz"), "wt") as f:
            for row in rows:
                f.write(json.dumps(row) + "\n")
    dist = collections.Counter(r["meta"]["chosen"] for r in train)
    print(f"train {len(train)} rows, eval {len(evals)} rows -> {a.out}")
    print("teacher's choices in train:", dict(dist.most_common()))


if __name__ == "__main__":
    main()
