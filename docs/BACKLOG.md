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
