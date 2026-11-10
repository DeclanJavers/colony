# Colony

An ant-farm colony sim where every colonist is an AI agent. **Laya** picks what each colonist
does, and **LFM3-1.7B** (a local LLM) lets you talk to any of them. Full design: [docs/SPEC.md](docs/SPEC.md). What's next: [docs/BACKLOG.md](docs/BACKLOG.md).

## Run it

```bash
# 1. AI services: Laya brain server (:8765) + LFM3 via llama.cpp (:8766). Laya runs on the CPU
#    (3 threads) and the chat model on the GPU, the split that keeps the game's frame rate best;
#    LAYA_DEVICE / LAYA_THREADS / LLM_GPU_LAYERS / LLM_THREADS change it (see run.sh).
brain_server/run.sh

# 2. Play it. It opens on the title screen: Continue, New game (seed and map options, with the
#    map regenerating behind the menu as you change them), Load game. Colonists are run by Laya;
#    the top bar shows whether it's connected.
#    Command-line options skip the menu: -- --seed 7 --prewarm 3 --speed 2 --brain utility
/Applications/Godot.app/Contents/MacOS/Godot --path .

# 3. Headless fast-forward (no window). Default is the hand-made baseline brain (fast);
#    --brain laya runs it on Laya through the brain server (about 10 decisions a second).
/Applications/Godot.app/Contents/MacOS/Godot --headless --path . -s res://tests/run_headless.gd -- --days 30 --seed 1234
/Applications/Godot.app/Contents/MacOS/Godot --headless --path . -s res://tests/run_headless.gd -- --days 3 --brain laya

# 4. Replay logged colonist states through Laya and compare with the baseline
brain_server/.venv/bin/python tests/smoke_brain.py logs/decisions/<run>.jsonl 30 --chat
```

Frame rate: the top bar shows FPS (Settings: Show FPS). `tools/perf.sh 12 --prewarm 30 --zoom 34`
measures it in a window on screen 0 (vsync off) and prints frame-time percentiles; test runs never
autosave over your game.

Requirements: Godot 4.6, `uv` (Python 3.12 venv in `brain_server/.venv`), `brew install llama.cpp`,
LFM3 GGUF at `~/.lmstudio/models/LiquidAI/LFM3-1.7B-GGUF/LFM3-1.7B-QAD-Q4_0.gguf`.

## The world

- **Seasons and weather** (`data/seasons.json`): four seasons of four days, rain, storms and snow.
  Warmth is a need; fires, the kitchen and heated homes (burning firewood) keep people warm.
- **Exploration:** colonists only use resources they've seen or been told about; unexplored land
  is under fog. They explore, and news of places spreads in conversation.
- **Tools** (`data/tools.json`): sticks and pebbles are picked up by hand; trees need a stone
  hatchet and big rocks a pickaxe, which people make and wear out. A rod lets them fish the river,
  and a spear lets them hunt.
- **Roles** (`data/roles.json`): nobody is assigned a job; people who've done a lot of something
  lately, and are among the best at it, become known for it (woodcutter, fisher, toolmaker...).
- **The weekly meeting** (`data/meetings.json`): the bell rings every 7th day; people gather at
  the meeting hall (or the campfire), share news, and agree a plan with volunteers. The hall is
  enlarged when people had to stand outside.
- **Terrain** (map options in `WorldMap.DEFAULT_OPTIONS`): a river, ponds, gentle hills, meadows.
- **Game animals** (`data/animals.json`, `scripts/sim/wildlife.gd`): rabbits in warrens and deer in
  herds live in the sim. They graze around a home spot that drifts, bolt from people who come close,
  and have young in spring and summer up to what the land carries. People remember where they saw
  game and pass it on in talk and at meetings. Hunters creep up and throw from close; a hit brings
  meat home. Every kill makes that kind warier for a while and moves the herd away, and hunted-out
  land slowly refills from the map's edge.
- **Scenery:** birds, fish jumping, butterflies, fireflies, falling leaves, rain ripples.
- **The story:** the Colony page keeps a timeline and a short summary of every week.
- **Look:** a pixel-art diorama; every new asset follows [docs/STYLE.md](docs/STYLE.md).

## How it's put together

| Folder | What lives there |
|---|---|
| `data/` | **All content.** Buildings, tiers, actions, needs, traits, nodes, names and tuning, as JSON. |
| `scripts/sim/` | Simulation core as plain data on a fixed tick (`Sim`, `WorldMap`, `Colonist`, `Building`, `ResNode`, `SimEvents`). No rendering. |
| `scripts/sim/actions/types/` | One file per action **type** (consume, gather, farm, build, sleep, rest, socialize, have_child). Found automatically. |
| `scripts/brain/` | `Brain` interface, `LayaBrain` + `LayaClient` (batched HTTP to the brain server), `UtilityBrain` baseline, `StateDescriber` (colonist → text for Laya). |
| `scripts/log/` | `DecisionLog` writes every decision and its outcome to `logs/decisions/*.jsonl` for fine-tuning. |
| `brain_server/` | Python FastAPI server: `/decide` (Laya), `/chat` + `/remember` (LFM3), `/health`. |
| `scripts/game/` | `main.gd` (entry point + screenshot hook), `SimRunner` (speed, pause, owns brains + log), `WatcherChat` (talking to colonists, chats → memories). |
| `scripts/view/` | `WorldView` (mirrors the sim in 3D), `IsoCamera`, `DayNight`. |
| `scripts/ui/` | `Hud` (time, population, stock, speed, event feed) and `Inspector` (colonist panel + chat). |
| `assets/models/` | Placeholder `.glb` models, generated by `tools/blender/make_models.py`. A building type with no model shows as a labelled box. |
| `tests/` | Headless runner, `systems_check` / `knowledge_check` / `save_check` / `overlap_check`, brain smoke test. |

## Which brain runs the colony

`data/config.json` → `decision.brain` is `"laya"` (default) or `"utility"`; `--brain` overrides it.
With Laya there are no rules on top: whatever it picks is what the colonist tries, and if the
choice can't be carried out the colonist hesitates and Laya chooses again. While a colonist waits
on Laya for more than `decision.max_lag_minutes` the sim slows down ("waiting on Laya..."), and if
the brain server is down colonists just keep thinking until it's back. `--prewarm` can't wait for
Laya, so skipped-ahead days run on the baseline brain. Click a colonist to see what its brain
weighed for its last choice.

## Playing

| Key / button | What it does |
|---|---|
| Click anything | Inspect it: colonists, buildings, sites, farms, trees, rocks, graves, ground |
| `C` / Colony | Colony overview: trends, stock, next tier, buildings, people, history |
| `Tab` / `Shift+Tab` | Next / previous colonist |
| `F` / double-click | Follow the selected colonist |
| `Space`, `1` `2` `3` | Pause, 1x / 3x / 10x speed |
| `F5` / `F9` | Save to this game's slot / load the latest save (an autosave is made every morning) |
| Settings | Who writes conversations (chat model for all / on-screen only / quick), names, speech bubbles |
| `Esc` | Close the panel; with nothing open, the pause menu (Resume, Save, Load, Main menu, Quit) |

**What you see on screen:**
- **Head badges:** a small icon over each colonist shows what they're doing. It turns red and pulses when they're in danger, and shows grey dots while their brain is thinking.
- **Hands:** they hold the tool for their job while working, and carry the goods they're bringing back.
- **Children** have bigger heads.
- **Little moments:** parents celebrate births, and builders cheer when they finish.
- **Signs of life:** houses glow in the evening while people are home, and the kitchen and workshop chimneys smoke.
- **Speech bubbles:** conversations show when you're zoomed in or a speaker is selected.

## Inspecting and talking to colonists

Click a colonist to open the side panel:
- **Status:** needs, family, feelings toward others, and which brain and model decide for them.
- **Last choice:** what the brain weighed for its last decision, with every option's probability.
- **Memories:** yellow ones are what Laya reads right now; blue ones came from talking to you.
- **Show what Laya reads:** the exact state text Laya gets.

At the bottom you can talk to the colonist as **the Watcher**. Chat runs through LFM3 (`/chat`).
When you leave a conversation, or every 6 messages, it's compressed into a one-sentence memory
(`/remember`, importance 7), and memories are part of the text Laya reads.
Laya v1 was trained only on the baseline, so it doesn't act on your requests yet: that needs
training data where Watcher requests matter (see the v2 plan in docs/BACKLOG.md). To test chat from the command line, use
`-- --select --say "your message"`.

## What colonists know, and how they talk

Colonists don't see the true stock. Each has a `Knowledge` (`scripts/sim/knowledge.gd`) of the stores:
- **How they learn:** what they've *seen* (near or at storage, or by choosing **check the stores**) and what they've *heard*.
- **Age:** every fact carries the time the information was originally observed, and memory blurs with age.
- **Trust:**
  - fresher information wins,
  - your own eyes beat hearsay,
  - friends are believed more than strangers.
- **What Laya reads:** these beliefs (with source and age) and any requests, not the truth.

**Conversations** happen when colonists socialize, when they chat casually while working side
by side, or when a colonist chooses **share news** or **ask for help**.
- **In the game:** the chat model writes the dialogue from each speaker's beliefs, needs and
  memories (`/converse`). A second step extracts what was actually told or asked, and the sim
  applies exactly that, mistakes included.
- **Headless and training:** a quick symbolic exchange does the same job.
- **Seeing it:** speech bubbles show when you're zoomed in or a speaker is selected. The inspector
  shows beliefs against the actual stock, requests, and recent conversations.
- **Requests:** asking someone, or telling a colonist something as the Watcher, creates a dated
  request that brains weigh against their own needs.

## Animations (NVIDIA Kimodo)

Colonists are the rigged villager (`tools/blender/villager.py`) animated with motions generated by
NVIDIA Kimodo from text prompts (`tools/animation/prompts.json`), retargeted onto the villager's
skeleton, made in place and looped smoothly.
- **Generate (Colab, T4 + High-RAM):** needs an `HF_TOKEN` secret with Llama 3 access. Mount Drive,
  clone and `pip install --no-build-isolation` Kimodo (with `libeigen3-dev`, `cmake`, `pybind11`),
  `pip uninstall torchao`, then `%run /content/drive/MyDrive/colony_anim/kimodo_colab.py`. That's
  about 30 s per clip.
- **Retarget and export:** `Blender -b -P tools/blender/villager.py -- --out assets/models --bvh assets/anim_src/bvh_v1 --takes tools/animation/takes.json`
  (add `--preview DIR` to render frames for picking takes).
- **In game:** `ColonistAvatar` plays the clip that matches each action; missing clips fall back to idle or walk.
- **Deer** are rigged and hand-keyed in `tools/blender/deer.py` (stag, doe, pale doe; idle, graze,
  walk, run): `Blender -b -P tools/blender/deer.py -- --out assets/models`. `WildlifeView` plays run
  when fleeing and walk when wandering (sped to match), and grazing with look-rounds at rest.

## Fine-tuning Laya

Base Laya can't run a colony (it ignores the state; 3 of 5 founders died by day 3). Fine-tuning
fixes that. Never train on the Mac: use Colab, ideally the cheapest GPU (T4).

1. Make baseline logs, e.g. 15-day headless runs on several seeds (`--pick sample` gives more
   varied states). Keep a couple of seeds for evaluation only.
2. `python3 tools/finetune/make_data.py --out logs/finetune/data_vN --eval-seeds 901,902 logs/decisions/<runs>.jsonl`
   turns them into Laya `{state, questions, gold}` rows, using the same questions the server
   asks (`brain_server/laya_format.py`).
3. Copy `data_vN/` and `tools/finetune/train_colab.py` to Google Drive `colony_laya/`, then in
   Colab: mount Drive, `!pip -q install laya==0.4.29`, `%run /content/drive/MyDrive/colony_laya/train_colab.py`
   (env `DATA=data_vN OUT=laya_colony_vN`). It prints held-out accuracy before and after.
4. Copy the checkpoint (without `checkpoint_latest/`) into `models/laya_colony_vN/`.
   `brain_server/run.sh` serves the newest `models/laya_colony_*` automatically.

v1 (24k baseline decisions, 2 epochs, 30 min on an A100): held-out agreement with the baseline
went from 42% to 85%. On seed 1234 it ran 6 days with no deaths, reached tier 4 on day 5, and had
the first birth.

## Extending it

- **Restyle models:** edit `tools/blender/make_models.py`, then run
  `/Applications/Blender.app/Contents/MacOS/Blender -b -P tools/blender/make_models.py -- assets/models`.
- **New building:** add an entry to `data/buildings.json` (tier, cost, work, footprint, `provides`).
  The `provides` keys the sim understands: `storage`, `beds`, `sleep_quality`, `rest_spot`,
  `water_source`, `farm{...}`, `colony{build_speed|food_yield|social_gain}`.
- **New action from an existing type:** add an entry to `data/actions.json`, e.g. another `gather`
  with different `sources`.
- **New kind of behaviour:** add `scripts/sim/actions/types/<type>.gd` extending `ActionBase`
  (`available` / `begin` / `tick` / `end`), then reference `"type": "<type>"` in actions.json.
- **New need or trait:** add it to `data/needs.json` / `data/traits.json`. Trait `mods` keys like
  `<need>_decay`, `work_speed`, `move_speed` and `carry` apply automatically.
- **New brain:** extend `Brain`, implement `decide(ctx, sim, done)`, and call `sim.register_brain()`.
  Brains are set per colonist (`colonist.brain_id`), so you can A/B test them in one colony.
- **Listen to anything:** connect to signals on `sim.events` (births, deaths, decisions, buildings, ...).
- **Change what Laya reads:** edit `StateDescriber` and bump its `VERSION` so logs stay comparable.
- **Swap the decision model:** set `LAYA_MODEL` / `LAYA_SUBFOLDER` before `brain_server/run.sh`
  (for example, a fine-tuned checkpoint).
