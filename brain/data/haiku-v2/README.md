# Haiku's decisions, v2 (2026-11-11)

6,840 decisions labelled by Claude Haiku through the subagent bridge (`colony-run label`), with
the M9 instructions (ARCHITECTURE.md section 8, step 4):

- **5,040 from rules colonies,** seeds 1 to 12, 48 days each, 420 sampled per colony
  (`runs/label-1`).
- **1,800 from colonies run by `scorer@haiku-v1`,** seeds 1 to 12, 24 days, 150 per colony
  (`runs/label-2`). This is the correction round: the situations a student trained on the first
  set gets itself into.

Seeds divisible by 5 are marked `held_out`.

- **`text.jsonl.gz`:** one decision per line: `prompt` (the words Haiku read), `choice` (the
  option number it picked, from 1), `reason` (its one sentence), `rules_choice` (what the rules
  brain would have picked), `split` and `seed`. This is for the text model and Laya.
- **`features.jsonl.gz`:** the same decisions as the scorer sees them (`{"split", "example"}`).
  Train with `colony-run train --examples`.
