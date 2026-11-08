# Backlog

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

## Readability: seeing what's going on

Done (2026-11-08):
- Click anything to inspect it: colonists, buildings and sites, farms, trees, rocks, bushes, graves, ground, and the Colony overview (button or `C`).
- Names in the panel are links that inspect the target and move the camera there.
- Hover tooltips.
- A colonist who dies while selected stays selected as their grave.
- Colony page: stock change over the last day, next-tier progress, buildings, a roster of every colonist with their task and worst need, graves, brain stats.

To do, in rough priority order:

**In the world**
1. **Head icons instead of text labels:** an action icon (axe, pickaxe, hammer, food, bed, speech) and danger icons (thirst, hunger). Show a "…" bubble while waiting on Laya and a "?" when Laya was unsure (low top probability). Show names only on hover, when selected, or when zoomed in. Fixes overlapping labels.
2. **Intent lines:** a faint line from a colonist to their target (tree, well, site), shown for the selected colonist or toggled for everyone.
3. **Carried items and tools:** a log, stone, berry basket or bucket while carrying; an axe, pickaxe, hammer or hoe while working. Possible now that the villager has hand bones.
4. **Floating popups:** "+6 wood" at storage, "Built!" and "Born!" on events.
5. **Progress over sites and fields:** progress bars on sites, a growth ring on farms, a sparkle when ripe.
6. **Visible resource levels:** trees and rocks shrink as they're used up, and berry bushes lose berries.
7. **Signs of life in buildings:** lit windows when a house is occupied at night, smoke when the kitchen is in use, a water level in the well.
8. **Hover and selection outlines** on 3D objects (outline shader).
9. **Night readability:** a rim light on colonists and a slightly brighter night ambient.
10. **Overlay modes:** colour by most urgent need, by job, relationship lines, an activity heatmap, reachable area.
11. **Minimap** with colonists as dots.

**Panels and camera**
12. **Event log:** clickable lines (inspect + focus), a scrollable full history, filters (births/deaths, building, chat), icons.
13. **Toasts for big events** (death, birth, tier, starvation warning), with an option to auto-slow or pause.
14. **Camera controls:** follow a colonist (double-click or `F`), `Tab` to cycle colonists, bookmarks.
15. **Colonist roster table:** sortable by name, task, each need, age, partner.
16. **Graphs:** population and each resource over time (hourly snapshots already exist in `SimRunner.history`), plus a births and deaths timeline.
17. **Tech-tier view:** every tier, what it needs, and what it unlocks.
18. **Per-colonist history:**
    - A timeline of their last N decisions, with Laya's probabilities and whether each worked.
    - Need sparklines.
    - Skills.
19. **Brain view:** colour colonists by Laya's confidence and highlight failed decisions. A "why not X?" view comparing Laya with the baseline for the same state.
20. **Relationship graph** (who likes whom, couples, families).
21. **HUD tooltips:** e.g. food → eaten per day, days left, top producers.
22. **Time controls:** show the real sim speed when Laya slows it; "skip to morning"; pause while typing to a colonist.
23. **Accessibility:** UI scale, font size, colour-blind-safe palette.

## Other known issues
- At 50 colonists Laya (~120-200 ms per decision; ~500-800 ms while the chat model and game share the GPU) can't keep up at 1x, so the sim slows down. Options: smaller or quantized model, `predict_batch` on matching question sets, or fewer decisions per colonist.
- Laya v1 had 11 failed decisions in 521 on seed 1234 (baseline: 0).
- Colonist labels overlap when colonists stand together.
- ObjectDB leak warnings at exit (harmless).
- The Godot AI MCP server never started on :8000, so everything is built from files.
