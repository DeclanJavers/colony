# The language model's fine-tuning set, v1 (2026-11-13)

Chat examples for fine-tuning LFM3-1.7B with LoRA (M11, `brain/lm/train_lora.py`): for every
language job the game has, the system prompt (instructions and the answer's JSON form, from
`brains::lm::system`), the user prompt (the situation, written by `sim::words` or the decision's
own text), and Claude Haiku's answer.

`lm.jsonl.gz`, one example a line: `task`, `split` (`train` or `held`, by seed), `seed`, `tick`,
`system`, `user`, `assistant` (Haiku's JSON answer), `code` (what the code version did: the
template line, the keyword reading, the rules' choice).

| Task | Train | Held out | What it is |
|---|---|---|---|
| goal | 800 | 160 | a goal for the coming days, from the goals that make sense now |
| talk | 792 | 160 | a talk between two people, 2 to 4 lines, from how it opened |
| memory | 800 | 160 | something that happened, in the person's own words |
| words | 800 | 160 | reading what the player said: actions, a goal, belief, a reply |
| step | 1,234 | 266 | the next step (Haiku's M9 decisions, `haiku-v4`) |

The goal, talk, memory and words jobs come from 24 rules colonies with goals on (seeds 101 to
124, 96 days each, 40 of each task per colony, `colony-run label --task goal,talk,memory,words`,
`runs/lm-label-1`), answered by Haiku through `tools/haiku_jobs` (344 calls on the Claude plan).
Every answer was checked the way the game checks the model's (valid JSON of the right shape, an
option that exists, lines by the two people talking, sentences of sane length): 3,832 of 3,840
jobs were answered, and all of those passed. Seeds that are multiples of 6 are held out.
The words jobs use `jobs::SAYINGS` (requests, goals, small talk, nonsense, lies and harmful
asks, with names filled in).
