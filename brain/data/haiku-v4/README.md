# Haiku's decisions, v4 (2026-11-11)

v3 (6,840 decisions; see `brain/data/haiku-v3/README.md`) plus **1,800 from a correction round
for the text model:** colonies run by `text-text_model_v1` (seeds 1 to 12, 12 days, 150
sampled per colony), labelled by Haiku (`runs/label-5`, 75 calls). Text model v1, trained on v3
alone, starved a colony within 24 days; these are the situations its own mistakes lead to.

The files are as in v3: `text.jsonl.gz` and `features.jsonl.gz`.
