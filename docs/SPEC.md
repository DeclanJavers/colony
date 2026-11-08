# Colony — design spec v0.1

A 3D isometric ant-farm colony sim. Every colonist is an AI agent. Laya chooses what each
colonist does, and a local LLM (LFM3-1.7B) lets you talk to any of them. You only watch,
talk, and control time. The base is built so that upgrades plug in later without rewrites.

Proposed location: `~/Desktop/Code/colony/`

---

## 1. Your decisions (from the Q&A)

| Topic | Decision |
|---|---|
| View | 3D isometric (orthographic camera), very basic models made in Blender |
| Player role | Watch, chat, and time controls (pause / 1x / 3x / 10x). No god powers |
| Chat | Click a colonist and chat in the side panel. What you say becomes a **memory** that Laya reads, so it nudges behaviour but doesn't override it |
| Scale | Starts with 5 colonists, can grow to about 50 |
| Births | Couples form through socializing. A child needs a free bed in a house and spare food, and grows into a worker in a few in-game days |
| Death | Starvation, thirst, and old age. A colony can collapse |
| Laya control | **Pure Laya, no safety net.** Every decision and its outcome is logged for fine-tuning later |
| Visibility | Action icon over every head. An inspector shows needs, memories, and Laya's probability for every option |
| Progression | Fixed tech ladder of about 10 buildings in tiers. Colonists choose what to build |
| Resources | Food, water, wood, stone, in one shared stockpile (everyone shares) |
| Individuals | Name, 1–2 traits, skills that improve with practice, short memories |
| Time & map | Day/night cycle (one day is about 2 min at 1x). Fixed ~60×60 map generated from a seed: river, forest, rocks, meadow |

## 2. Architecture (built to be expanded)

```
┌──────────────────────── Godot 4.6 ────────────────────────┐      ┌──── brain_server (Python) ────┐
│                                                           │      │                               │
│  Registry (autoload)  ← data/*.json  (all content)        │      │  POST /decide  → Laya         │
│  Sim (pure data, fixed tick, seeded, headless-capable)    │ HTTP │  POST /chat    → LM Studio    │
│    ├ World / Map / Stockpile / Buildings                  │◄────►│  POST /remember→ LM Studio    │
│    ├ Colonists (needs, traits, skills, memory, relations) │      │  GET  /health                 │
│    └ ActionRunner  (walk → work → apply effects)          │      │  (localhost only)             │
│  Brain interface  ← LayaBrain | UtilityBrain | Scripted   │      └───────────────────────────────┘
│  StateDescriber   (colonist → ≤300-token text)            │                    │
│  EventBus (signals) → Views, UI, DecisionLog, upgrades    │          LM Studio :1234 (LFM3-1.7B)
│  Views (3D, read-only on the sim)  /  UI (inspector, chat)│
└───────────────────────────────────────────────────────────┘
```

**Rules that keep it expandable:**

1. **All content is data.** Resources, needs, actions, buildings, tiers, and traits are defined in
   `data/*.json`. To add a building, write one JSON entry and add one `.glb` model. No code change.
2. **A few generic action types, many actions.** Actions such as `gather`, `consume`, `build`,
   `work_at`, `rest`, `socialize`, and `have_child` are scripts. JSON fills in their parameters.
   For example, "chop wood" is `gather{resource: wood, source: tree}`. A new kind of behaviour
   is one new script that is registered by name.
3. **The brain sits behind an interface.** `Brain.decide(context) -> Decision{choice, probs}`.
   It can be swapped per colonist, so you can run half the colony on base Laya and half on a
   fine-tuned version and compare them side by side. `UtilityBrain` is a hand-tuned baseline
   that also lets the sim run with no Python at all.
4. **The sim never touches rendering.** It is plain data on a fixed tick with a seeded RNG.
   Views only read it. This allows headless fast-forward runs, automated tests, cheap
   generation of fine-tuning data, and save/load later.
5. **An event bus connects everything.** Births, deaths, decisions, finished buildings, and
   chats are signals. The UI, the logger, and future upgrades subscribe to them instead of
   being wired into the core.
6. **The prompt builder is its own component.** `StateDescriber` turns a colonist into text.
   You can swap or version it and iterate on prompt formats without touching anything else.

## 3. How a colonist decides (Laya)

The colonist finishes its current action and becomes idle. Then:

1. `StateDescriber` writes a compact situation (the English Laya checkpoint fits about 320 tokens):
   name and traits, needs in words ("very thirsty, a bit tired"), time of day, shared
   stockpile, what is unlocked or unbuilt, and the top 3 memories by importance and recency.
2. The **available** actions become the options, about 12 at most (Laya gets less accurate
   above ~20). Each option has a one-line description.
3. **One Laya call with several questions**, all answered in a single forward pass:
   - `action`: which action to take
   - `build_what`: which building, if building
   - `with_whom`: which colonist, if socializing
4. Laya picks the highest-scoring option. A config setting allows weighted random picks instead.
5. The choice and the full probability table go to the inspector and the decision log.

**Throughput:** decisions happen only when a colonist becomes idle, not every frame. Requests
are queued and batched. A colonist shows a "thinking" icon until the answer comes back. If Laya
can't keep up at 10x with 50 colonists, the sim slows down rather than skipping decisions.

**Decision log for fine-tuning** (`logs/decisions/<run_id>.jsonl`): one line per decision with
the state text, the options, the probabilities, the choice, and the brain/prompt version. An
outcome is added later: need levels N in-game hours afterwards, and whether the colonist
died within one day. The LLM, or simple rules, can later label each decision good or bad to
build the fine-tuning set. The colonist's personal seed is stored too, so runs can be replayed.

## 4. Chat (LFM3-1.7B via LM Studio)

- The system prompt comes from a snapshot of the colonist: name, traits, needs, current
  action and why (the top Laya probabilities), memories, and relationships. Replies are short
  and in character. Reasoning ("thinking") is switched off for speed.
- When the chat closes, the LLM boils it down to a single memory sentence, for example
  *"The watcher told me we need a well."* That memory is fed into future Laya decisions.
- The sim keeps running while you chat. You can pause it yourself.

## 5. Starting content (v1)

**Needs:** hunger, thirst, energy, and age. A need left at 0 for a while means death. Sleeping
in a house restores more energy than sleeping outside.

**Natural sources:** river (water), berry bushes (food), trees (wood), rocks (stone).

| Tier | Buildings | Unlocks when |
|---|---|---|
| 0 | Campfire, stockpile | start |
| 1 | Well, farm plot | a campfire exists |
| 2 | House (2 beds, needed for births), storehouse | a well and a farm exist |
| 3 | Workshop (faster building), kitchen (more food per crop) | 2 houses exist |
| 4 | Town hall (meeting point that boosts socializing) | workshop + kitchen exist |

**Actions:** drink, eat, sleep, gather (food / wood / stone / water), farm (plant/tend/harvest),
build, socialize, have a child, idle/wander.

**Traits (first set):** hardworking, lazy, social, loner, cautious, curious. Each is a sentence
Laya reads, plus small numeric modifiers.

## 6. Graphics (placeholder, readable)

- Orthographic isometric camera with pan, zoom, and rotation in 90° steps.
- **Blender models are generated by a script** (`tools/blender/make_models.py`, run headless),
  so they can be rebuilt and restyled with one command. Colonists are coloured capsules with
  a head (children are smaller). Each building has a simple colour-coded shape. Trees are
  cones on cylinders. Rocks are grey lumps.
- Tiles show grass, water, and dirt/farmland.
- A floating icon/label above each head shows the current action. Selecting a colonist
  highlights them with a ring.

## 7. Build order

0. **Setup:** project skeleton, Python brain server (uv, Python 3.12), install Laya, download
   LFM3-1.7B in LM Studio, connect the Godot MCP.
1. **Headless sim core:** registry, map, needs, actions, stockpile, building, with UtilityBrain.
   Test: 5 colonists survive and build a well in a headless run.
2. **3D view:** Blender models, map, camera, day/night, time controls.
3. **Laya brain:** StateDescriber, `/decide`, queueing, decision log. Compare it against
   UtilityBrain over a few headless days.
4. **Inspector + head icons.**
5. **Chat + memory.**
6. **Life cycle:** relationships, couples, births, aging, death.

## 8. Upgrades for later

Planned now, built later: fine-tuning pipeline from the logs, seasons and winter, god powers,
crafted goods (planks, bricks, tools), buildings invented by the LLM, save/load, colonists
talking to each other through the LLM, multiple colonies, A/B tests between brains, and
better art.

## 9. Known risks

- **Base Laya may be poor at this zero-shot.** Convai reports 0.36 accuracy on typed decisions
  before fine-tuning, so early colonies may die. That's expected, and it's why the decision
  log exists.
- **Laya's speed on Apple silicon (MPS) isn't documented.** CPU is about 0.2–0.5 s per call, so
  10x speed with 50 colonists may hit the throughput limit.
- **LFM3's on/off switch for "thinking" is described as inconsistent.** Its replies may need
  their `<think>` blocks stripped out.
