# Backlog

## Overnight queue (2026-11-09)

Decided with the user:
- **Animals are scenery for now** (birds, rabbits, deer wander, graze, flee). Hunting comes later as its own pack.
- **Tools are needed for big resources.** Start with sticks and small stones from the ground, craft a stone hatchet and pickaxe, which are required to fell trees and break big rocks. Tools wear out.
- **Fishing** from the river shore with a stick rod.
- **Ambient life:** butterflies, fireflies, falling leaves, rain ripples. Looks only.
- **Terrain:** raised plateaus with stepped earth edges, small ponds, flower and tall-grass patches, more varied rocks.
- **Game shell:** a menu with New Game (map generation options), save and load slots, and the whole thing looking beautiful.
- Also: polish from the checker notes, tests for the new systems, docs refresh.

Order: v4 scoreboard first, then features, then the v5 training data last, so it covers everything. Each step gets an independent check.

**Done overnight (2026-11-09):** tools (sticks, pebbles, hatchet, pickaxe, wear), fishing, wildlife and ambient life, hills/ponds/meadows with map options, title menu with new-game options, save slots with thumbnails and an autosave, pause menu, polish (names in crowds, night glow, vignette, hall near the village, meetings start when people have arrived), `tests/systems_check.gd`.

**Next:**
- **Hunting pack (done 2026-11-09):** rabbits and deer live in the sim (`scripts/sim/wildlife.gd`, `data/animals.json`): herds and warrens graze, flee, breed in spring and summer up to the land's capacity, and refill from the map's edge when hunted out; kills make that kind warier for a while. Spear (`data/tools.json`), `hunt` action (`scripts/sim/actions/types/hunt.gd`): track a sighting, stalk, throw, carry the meat home. People remember and share sightings. Teacher weighs hunting as a food job adjusted by how well it pays (a deer ≈ 1.6× berries per hour). New Game option "Deer and rabbits". Hunter role. State text v8 (`Game: ...`). In 16-day teacher colonies (seeds 1234/77/4242): 7, 4 and 0 hunts (berries and farms usually cover food in the first year), roughly a third to half succeed, herds stay near capacity, no deaths. An independent check found and fixed: hunters fixating on animals they couldn't reach, animals stuck where buildings went up (or old saves put them), the wrong sighting being forgotten, meat lost when the bell interrupted the walk home, and freshly spooked deer being impossible to reach (now a hurried throw as they bolt).
- **Performance pass (2026-11-09):** FPS readout in the top bar; `tools/perf.sh` measures frame times. Laya asks the action first and the follow-up (what to build, who with) only when needed: same answers, ~3x less work. Laya moved to the CPU (3 threads; on the GPU it cost 30-50 fps), the chat model stays on the GPU. Quantizing Laya was tried and dropped: 8-bit ONNX agreed with the full model on only 61/80 decisions and was no faster, PyTorch's 8-bit doesn't run on Laya's model, and the Neural Engine route failed to build. Sim: building-spot search uses summed-area tables (a meeting hall placement 105 -> ~4 ms), place lookups indexed by type, native sorts, warm spots cached per tick, decisions limited to ~2 ms per frame in the game, the decision log writes a few lines per tick, the autosave writes on a worker thread. Trees, rocks and litter are drawn in MultiMeshes. Result: ~140 fps in normal colonies with both AI models running (was 38-90); a 50-person colony zoomed out with Laya and constant chat is ~100-110 fps (the chat model generating nonstop is the main cost there; Settings -> Conversations "Only ones on screen" lightens it). Crash on quit (the autosave's worker task was never waited on) fixed.
- **Still slow-ish:** a single colonist decision is ~3 ms in a big colony, so frames with one go past 8.3 ms (120 Hz). Splitting a decision across frames, or making `available_actions` cheaper (each gather action still searches sources), would fix that.
- **Sleep drifts into the day (found while testing hunting):** sleep isn't anchored to night (only a night bonus), so a tired colony can nap through the 17:00 meeting bell, and the bell doesn't wake anyone. On seed 99 the day-7 meeting had one person and wasn't held. Worth a look in the teacher (sleep timing) or the bell.
- **Laya and hunting:** v5 was trained before hunting and on state text v7, so it won't hunt and the new `Game:` line is unfamiliar to it. A v6 round (teacher colonies in the hunting world plus v5's logged mistakes from `logs/eval/*_v5.json` runs) would fix that.
- **Laya v5 is in and is the default brain again.** Trained 2026-11-09 on `data_v5` (61 min on an L4; no T4 was free). Held-out agreement with the teacher 74% → 79%. Scoreboard (12 days × seeds 1234/77/4242, `logs/eval/*_v5.md`): no deaths, 8 people on average vs 7 for the hand-made brain, 3 births, makes hatchets, pickaxes and rods, and fishes.
- **v5's weak spots (for v6):** too many farms on seed 1234 (3.2 per person); 2% failed decisions vs 0%; a bit more homelessness (25% vs 18% of the time); follows fewer Watcher requests (45% vs 65%); explores a little less. Its scoreboard runs logged the teacher's answers, so they're ready to use as v6 mistake data.
- **Explore is rare in the teacher's data now** (12 of 21k rows): with tools and fishing it seldom needs to. Worth a look if v5 never explores.

## Next big Laya training run (v2)

Goal: one larger fine-tune that covers everything below, instead of several small runs.
v1 (2026-11-08) was imitation only: 24k baseline decisions, 2 epochs, held-out agreement 85%.
It keeps the colony alive but is capped at the baseline's quality and ignores the Watcher.

### 1. Act on what the Watcher says
- **Problem:** chat memories reach Laya's state text, but v1 ignores them. A memory saying "The Watcher asked me to fetch water" moved "fetch water" only from 0.4% to 0.9%.
- **Data:** take baseline states, insert a Watcher memory that asks for an action, and shift the target toward that action. Request phrasings should vary (LFM3 `/remember` output is the realistic source). Include requests that can't be done right now, such as building something not yet unlocked.
- **Design decision first: how obedient are colonists?**
  - Always obey.
  - Obey unless a need is urgent (very thirsty, starving, exhausted).
  - Depends on traits, for example a loner or lazy colonist listens less.
  This decides the target weights.
- **Memory fading:** decide whether old requests should stop mattering. Memories already fade with a one-day half-life in `top_memories`.

### 2. Learn from outcomes, not just the baseline
- Every logged decision already has `outcome` (needs 6h later, alive or dead).
- Reweight or filter training rows by outcome, so Laya can beat the baseline instead of only copying it.
- Possible colony-level reward: survival, tier speed, births, buildings per day.

### 3. Train on Laya's own mistakes (DAgger-style)
- v1 only saw states from healthy baseline colonies, so if Laya drifts into a crisis (no water, starving) it has no examples to go on.
- Run colonies on Laya and have the baseline score each state Laya reaches. Log that as a teacher label next to Laya's own choice, for example a `teacher_probs` field added in `Sim._request_decision` when running Laya. Then retrain on those states.
- Add harsh starts too (low stock, bad seeds, a drought) for crisis coverage.

### 4. Data quality fixes to do before generating v2 data
- **Indistinguishable social options:** early on, every option reads "someone you barely know, nearby", so the who-to-socialize question can't be learned. Add more to `relation_text`, such as affection, last time together, or partner potential.
- **Missing numbers:** need words hide exact values, so the baseline's choices aren't fully learnable from text. Consider more need levels or rough numbers in `StateDescriber`, and bump `VERSION`.
- **Overbuilt houses:** the baseline builds too many houses late (22 houses for 18 people), so tune it before distilling.
- **State text length:** watch the ~320-token budget once Watcher memories are added.

### 5. Run settings
- Use the cheapest Colab GPU (T4), not an A100. Size micro-batches for it and use fp16 (no bf16).
- More data: more seeds and longer runs, aiming at the full ~38k+ rows plus the new request and outcome rows.
- Keep held-out seeds. Report agreement plus colony outcomes (survival, tier speed, births) against both v1 and the baseline.

## Knowledge and communication

**Done (2026-11-08):**
- Beliefs and perception, plus the "check the stores" action.
- Trust-weighted hearsay and natural misremembering.
- Chat-model conversations with grounded fact and request extraction.
- "Share news" and "ask for help" actions; requests from colonists and from the Watcher.
- Laya v2 trained on all of it.

**Next:**
- **Exaggeration and lies:** the hook is `Knowledge.tell(..., distort)`. Give colonists motives first: competition, self-interest, blame, rivalries.
- **Wider beliefs:** extend them beyond stock to ripe farms, unfinished sites, who's hungry, and where resources are.
- **Settings:**
  - When the chat model writes conversations: all, near the camera only, or never.
  - Whether action availability should use beliefs instead of truth. Today the options offered still leak which sources physically exist.
- **Outcome-based training,** so Laya can learn when trusting news pays off.

## Readability: seeing what's going on

Guiding rule (from the user): keep it simple, easy to read and good-looking. Show detail on demand
(hover, select, inspector), and don't pile features on screen.

**Done:**
- Click anything to inspect it: colonists, buildings and sites, farms, trees, rocks, bushes, graves, ground, and the Colony overview (`C`).
- Names in the panel and the event feed are links.
- Hover tooltips.
- Round icon badges over heads: one style, soft colours. A red pulsing badge means danger; grey dots mean the brain is thinking. Names appear only when zoomed in, hovered or selected.
- A dotted line from the selected colonist to their target.
- Carried goods are visible.
- Soft "+N resource" popups at storage.
- Site percentages, golden ripe crops, and trees and rocks that shrink as they're used.
- Camera: `Tab` cycles colonists, `F` or double-click follows, and the camera glides instead of snapping.
- Recent choices on the colonist page.
- Slightly brighter nights.
- Tools in hand while working; trend lines (people, food, water, wood, stone) on the Colony page.

**Maybe later (only if they stay clean):**
- A single optional overlay toggle (e.g. colour colonists by their most urgent need).
- Lit windows at night in occupied houses.

**Left out on purpose (clutter):** minimap, many overlay modes, a roster table (the Colony page covers it), toasts, relationship graph, tech-tree screen.

## Other known issues
- At 50 colonists Laya (~120-200 ms per decision; ~500-800 ms while the chat model and game share the GPU) can't keep up at 1x, so the sim slows down. Options: smaller or quantized model, `predict_batch` on matching question sets, or fewer decisions per colonist.
- Laya v1 had 11 failed decisions in 521 on seed 1234 (baseline: 0).
- ObjectDB leak warnings at exit (harmless).
- The Godot AI MCP server never started on :8000, so everything is built from files.
