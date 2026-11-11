# Haiku's decisions, v3 (2026-11-11)

The same 6,840 decisions as v2, with newer words: a text brain is now told about family
(partner, children) and farming (fields, seed grain, and that grain sown in spring feeds the
winter), and the instructions say these people mean to put down roots and raise families.

- **2,635 have new answers:** every decision that offered farming or family (clearing, sowing,
  tending or harvesting a field; asking for a partner or a child, or answering), and a quarter of
  those that offered courting, relabelled by Haiku with the new words (`runs/label-3`,
  `runs/label-4`, 112 calls).
- **The rest keep Haiku's v2 answers** with the new words; 24 of the relabelled answers were
  unusable, and those keep their v2 answers too. Merged by `brain/merge_labels.py`.

What changed when relabelled, on the same decisions:

| Option | Chosen before | Chosen now |
|---|---|---|
| Clear a field | 1% | 3% |
| Ask a partner for a child | 0% | 4% |
| Say yes to a child | 4% | 31% |
| Ask someone to be your partner | 0% | 43% |

A scorer trained on it (`colony-run train --examples`) keeps 19.7 people alive after two years,
with 10 born (v2: 9.9 alive, none born), but still farms nothing.

The files are as in v2: `text.jsonl.gz` (prompt, choice, reason, rules_choice, split, seed,
person, tick) and `features.jsonl.gz` (`{"split", "person", "tick", "example"}`).
