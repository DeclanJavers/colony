#!/usr/bin/env python3
"""Merge labelling runs into one dataset (M9).

A relabel run (`colony-run label --only ...`) answers again a part of an earlier sample, with
newer words. This writes the earlier sample with the newer words throughout (from a run made with
`--no-driver`, which costs nothing), Haiku's new answer where it relabelled, and its old answer
elsewhere. Decisions are matched by (seed, person, tick).

  python3 brain/merge_labels.py OUT  BASE_OLD:BASE_NEW[:RELABEL] ...
  e.g. python3 brain/merge_labels.py brain/data/haiku-v3 runs/label-1:runs/label-1b:runs/label-3 runs/label-2:runs/label-2b:runs/label-4

BASE_OLD has the answers (dataset/ and requests.jsonl, in the same order), BASE_NEW the same
decisions with the new words (requests.jsonl), RELABEL the new answers for some of them.
"""
import gzip
import json
import os
import sys


def key(t):
    r = t["request"]
    return (t["seed"], r["person"]["index"], r["tick"])


def lines(path):
    with open(path) as f:
        return [json.loads(l) for l in f]


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        sys.exit(2)
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    text_out = gzip.open(os.path.join(out, "text.jsonl.gz"), "wt")
    feat_out = gzip.open(os.path.join(out, "features.jsonl.gz"), "wt")
    total = relabelled = 0
    for spec in sys.argv[2:]:
        parts = spec.split(":")
        old, new = parts[0], parts[1]
        re_dir = parts[2] if len(parts) > 2 else None
        old_reqs = lines(f"{old}/requests.jsonl")
        old_text = lines(f"{old}/dataset/text.jsonl")
        old_feat = lines(f"{old}/dataset/features.jsonl")
        assert len(old_reqs) == len(old_text) == len(old_feat), f"{old}: every decision must have an answer"
        new_reqs = {key(t): t for t in lines(f"{new}/requests.jsonl")}
        fresh = {}
        if re_dir:
            # Matched by the person and tick each row carries (answers that were unusable are missing).
            feats = {(f["example"]["seed"], f["person"], f["tick"]): f for f in lines(f"{re_dir}/dataset/features.jsonl")}
            for a in lines(f"{re_dir}/dataset/text.jsonl"):
                k = (a["seed"], a["person"], a["tick"])
                fresh[k] = (a, feats[k])
        for t, a, f in zip(old_reqs, old_text, old_feat):
            k = key(t)
            n = new_reqs[k]
            if k in fresh:
                a, f = fresh[k]
                relabelled += 1
            row = dict(a)
            row["prompt"] = n["request"]["text"]
            text_out.write(json.dumps(row) + "\n")
            feat_out.write(json.dumps(f) + "\n")
            total += 1
    text_out.close()
    feat_out.close()
    print(f"wrote {out}: {total} decisions, {relabelled} with new answers")


if __name__ == "__main__":
    main()
