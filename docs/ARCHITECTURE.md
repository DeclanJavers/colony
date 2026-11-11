# Colony: architecture

How the base is built. What it's for is in [VISION.md](VISION.md); the order of work is in
[ROADMAP.md](ROADMAP.md). Library choices are proposals: versions are pinned at M1.

## 1. Shape

Everything is Rust, in one Cargo workspace. Python is used only for training models offline.

```
                ┌──────────────── colony (the game app: winit window) ────────────────┐
                │  render (wgpu)   ui (our own kit)   input   audio (later)           │
                │        ▲ reads snapshots          │ sends commands                  │
                └────────┼──────────────────────────┼─────────────────────────────────┘
                         │                          ▼
   ┌─────────────── sim (pure data, deterministic, no graphics) ───────────────────────┐
   │  world · people · actions · knowledge · events · chronicle · saves · packs        │
   │                 │ decision requests          ▲ decisions                          │
   └─────────────────┼────────────────────────────┼────────────────────────────────────┘
                     ▼                            │
   ┌──────────── brain (one plug, many backends) ─┴───────────────────────────────────┐
   │  rules · scorer · text model · subagent bridge · laya adapter · chat              │
   │  → every decision writes a trace                                                  │
   └───────────────────────────────────────────────────────────────────────────────────┘
   colony-run (headless CLI): runs, arena, branching, subagent runs, data export, maps
```

**Crates:**
- `crates/sim`;
- `crates/brain`;
- `crates/render`;
- `crates/ui`;
- `crates/colony` (the app);
- `crates/colony-run` (the headless CLI);
- `crates/trace` (the trace format and reader).

**Content** lives in `data/base/*.ron`. Each expansion later adds `packs/<name>/`.

**Rules:**
1. **The sim never knows about rendering, UI or a window.** It runs headless at full speed for
   tests, training runs and the arena.
2. **The view only reads.** The app reads sim snapshots and sends commands (god powers, settings,
   speed). It never changes sim data directly.
3. **Content is data where it can be.** Resources, items, buildings, crops, animals, needs,
   skills, traits, genes and action parameters live in RON files. Code holds a small number of
   generic action kinds that data fills in, for example `gather{resource: wood, source: tree}`.
4. **The brain is behind one interface,** swappable per person (see section 6).
5. **Everything that happens is an event.** The chronicle, the story feed, memories, stats,
   traces and expansions all listen to events instead of being wired into systems.

## 2. Simulation core

- **Time:** 1 tick = 1 game minute.
  - **Day and year:** a day is 1,440 ticks. The year length is data (default 4 seasons × 6 days),
    so a whole life fits in a few hours of play.
  - **Game speeds:** 1×, 3×, 10× and max.
- **Determinism.** The same seed, data, brains and command log always give the same world.
  - **Random numbers:** each system has its own seeded random stream. We use our own small
    generator (xoshiro256**, seeded through SplitMix64), so a library update can never change a
    world.
  - **Iteration order:** never depends on hash-map order. Use `Vec`, `BTreeMap` or IndexMap.
  - **Parallel work:** computed in parallel (rayon), applied in a fixed order.
  - **Checking it:** a hash of world state is checked by tests.
  - **What it enables:** saves you can branch, fair brain comparisons, and replays.
- **Data store:** typed ids (`PersonId`, `BuildingId`…) into tables, struct-of-arrays where it's
  hot, generational ids so stale references fail safely. No general ECS library: expansions add
  their own tables keyed by id (section 10).
- **Threads:**
  - **Sim thread:** the sim runs on its own thread at a fixed tick rate.
  - **Snapshots:** after each tick it publishes a snapshot (double-buffered) that the renderer
    interpolates between.
  - **Commands:** commands from the app go into a queue, are applied at the start of a tick, and
    are recorded in the command log.
- **Saves:** sections per system and per pack, each with its own version, written with postcard +
  zstd.
  - **Background saving:** writes go to a temp file that's renamed when done, on a background
    thread that is always waited on before quit. That was v1's quit crash.
  - **Branching:** loading a save and continuing with different commands or brains is a branch.

## 3. World

- **Grid:** 1 tile = 1 m. Chunks are 64×64 tiles. The base region is 256×256 tiles (configurable
  up to 512). Chunks keep the door open to bigger worlds and to simulating far chunks in less
  detail later.
- **Per-tile fields:**
  - height level;
  - terrain;
  - biome;
  - water (sea, lake or river, with flow direction);
  - soil moisture;
  - fertility;
  - temperature (from climate, season and height);
  - footfall (wear);
  - beauty (sum of nearby beautiful things);
  - reserved: owner, pollution.

  These are the quantities expansions later change (irrigation raises moisture, manure raises
  fertility, and so on).
- **Generation:** a pipeline of named stages, run in order from the seed:
  1. continents and sea;
  2. mountains and hills;
  3. light erosion;
  4. rivers that run downhill into lakes and the sea;
  5. climate: temperature from latitude and height, rain from wind and mountains' rain shadow;
  6. biomes;
  7. soil;
  8. plants and resource nodes;
  9. animals;
  10. the start site.

  Each stage is a function; packs can insert stages. `colony-run map --seed N` writes PNGs of
  each layer so a seed can be judged without starting the game.
- **Lookups:** pathfinding is A* on the tile grid with costs.
  - **Trails:** cheaper, and they pull traffic onto them early, so paths form.
  - **Rivers:** can be waded slowly; fords and bridges (an expansion) make crossing fast.
  - **Lakes and the sea:** block. There's a cache for repeated routes, and spot
  searches use summed-area tables (v1 lesson: building placement went from 105 ms to 4 ms).

## 4. People

Each person carries the following:

| Part | What it holds |
|---|---|
| Identity | Name, sex, birth date, parents, children, home, group |
| DNA | Genes with two alleles each. Expressed: height, build, skin, hair and eye colour, and personality leanings. A child takes one allele from each parent, with rare mutation. Gene definitions are data. |
| Body | Age, health, injuries, pregnancy. Reserved for expansions: illness, fitness. |
| Needs | Hunger, thirst, rest, warmth, social, fun. Each drains and is filled by actions; the rates are data. |
| Mood | Built from needs, recent memories, surroundings (beauty, crowding) and relationships. Each part is listed so the UI can say why. |
| Personality | Five traits: curiosity, diligence, sociability, warmth, calm. From DNA plus life events; they drift slowly. |
| Skills | Experience per skill, grown by doing and by being taught. |
| Knowledge | Facts ("berries at the east grove", "Tobin is a good hunter"), each with its source, when it was learned and how much it's trusted. Recipes known. Knowledge spreads by telling and teaching, and dies with the person. |
| Memory | Episodes they saw, did or heard about, with feeling and importance. They fade unless important. |
| Relationships | Per pair: familiarity, affection, respect, kinship, partner. They change through shared time, help, conflict and gossip. |
| Leisure | What they enjoy, and hobbies that emerge from repeated enjoyable activity. |
| Beliefs | A store of belief records. The base uses it only for world beliefs; religion and politics expansions add more. |
| Current | Action, target, plan, and the decision that started it. |

## 5. Actions

- **What each action kind defines:**
  - when it's possible;
  - how to list concrete options (the action plus a target);
  - **features:** the numbers the scorer reads;
  - **text:** the line text brains read;
  - **steps:** walk, work, finish;
  - **effects and outcome.**
- **Options are concrete:**
  - Example: "Forage at the east grove (120 m, plenty, known 2 days ago)", not "forage" and then
    a follow-up question.
  - Each action offers its best few targets (up to 3) by distance and value, so a decision has
    roughly 10–30 options.
  - Actions that aren't possible are listed with the reason ("farm: no seeds known") in the
    trace, not silently dropped.
- **The base action list:**
  - **Survival:** drink, fetch water, forage, eat, sleep, keep warm.
  - **Materials:** chop wood, gather sticks, gather stone.
  - **Food and fire:** make fire, cook, hunt.
  - **Building:** build (shelter, house, storage, fire pit).
  - **Farming:** clear, plant, tend, harvest.
  - **Social:** talk, share news, ask for help, teach, look after a child, court.
  - **Leisure:** rest, walk, play, and hobbies: garden, carve, sing.
  - **Beauty:** plant flowers, carve a statue, decorate a home, lay stones.
  - **Other:** check the stores, explore.
- **Beauty has an experiment switch:** "beauty matters" on or off.
  - **On:** beauty raises the beauty field, others notice it, and it lifts their mood and goes
    into their memories.
  - **Off:** it has no mechanical effect at all. Any brain that still makes beautiful things is
    choosing beauty for its own sake.

## 6. Brains

```rust
trait Brain {
    fn id(&self) -> BrainId;                       // e.g. "scorer@v3"
    fn submit(&mut self, req: DecisionRequest);    // may answer now or later
    fn poll(&mut self) -> Vec<Decision>;           // finished decisions
}
// Decision { request, choice, scores: Vec<f32>, reason: Option<String>, latency, fallback: bool }
```

**Backends:**

| Brain | What it is | Speed | Role |
|---|---|---|---|
| `rules` | Hand-written scores over the features | Instant | Fallback, Low setting, first teacher |
| `subagent` | Claude Haiku 5.5 as Claude Code subagents, through the bridge (section 8) | Seconds, sim paused | Main teacher, "watch Claude play" |
| `scorer` | Small model over the features (gradient-boosted trees or a tiny MLP), run natively in Rust | Under 1 ms | Default for most people |
| `text` | Our own small text model: a 20–100M-parameter encoder distilled from subagent decisions, run through ONNX Runtime (`ort`) | ~5–20 ms | People where words matter |
| `laya` | Adapter to the Laya server from v1 (HTTP); fine-tuned now and then on new traces | ~400 ms | Comparison |

**Assignment:** any brain can be assigned per person, per family or per group, and changed at any
time. The person page shows which brain is deciding.

**As built (M3):**
- **The rules brain** lives in the sim crate, because the sim always needs it as the fallback
  and the second opinion. Other brains live in their own crate.
- **No brain state:** the rules' small random noise comes from the seed, the person and the
  tick. So brains hold no state that saves would need: the same request always gets the same
  scores, and continuing after a load matches continuing without.
- **Options come only from what a person believes.** They see 12 tiles around them, and what
  they saw can be out of date. Old sightings count for less, because others may have picked
  that bush since.

**Modes:**
- **Live (normal play).**
  - **Time limit:** brains have a per-frame time limit (v1: 2 ms).
  - **Thinking:** a person waiting on a slow brain shows they're thinking and keeps their current
    action.
  - **Fallback:** if the answer is very late, `rules` decides and the trace says so.
- **Lockstep (headless runs, arena, subagent).** The sim waits for every decision, so results are
  deterministic and waiting costs nothing.

**AI budget setting (Settings → AI).** Each job has a ladder of backends; presets pick a rung for
each, and every job can be set on its own.

| Job | Low | Medium | High | Max |
|---|---|---|---|---|
| Everyday decisions | scorer | scorer | text for named people, scorer for the rest | text, or subagent (paused) |
| Conversations | templates | local LLM, on screen only | local LLM, everyone | subagent (paused) |
| Your words → requests | keyword matching | local LLM | local LLM | subagent |
| Thoughts on the person page | templates from the decision's top features | local LLM | local LLM | subagent |

- **Every job has a code fallback.** The game is fully playable on Low with no servers running.
- **Default preset:** picked from the machine (RAM, GPU), and you can override it.
- **Local LLM:** llama.cpp's server (OpenAI-style HTTP, localhost); v1 used LFM3-1.7B Q4_0 at
  ~80 tok/s on the M3 Pro's GPU.

## 7. Decision traces

Every decision by every brain writes a trace, and the trace is also the training row: what we
inspect is exactly what we train on.

**A trace holds:**
- **Who and when:** run id, seed, tick, person, brain id and version, mode.
- **What the brain saw:**
  - the named features with values;
  - the exact text sent, for text and subagent brains;
  - for each option: its features, its text, and whether it was possible (with the reason if not).
- **What it picked:**
  - the score or probability for every option;
  - the choice;
  - the reason it gave (subagent and LLM brains);
  - the latency, and whether it was a fallback.
- **A second opinion:** `rules` always scores the same request alongside (it's nearly free), so
  every trace shows where a brain disagrees with the rules.
- **What happened next,** filled in later: finished, failed or interrupted, how long it took,
  and how the needs changed.

**Storage:**
- **Headless runs:** traces are written to `runs/<id>/traces/*.jsonl.zst`, per run.
- **Normal play:** the last 200 per person are kept in memory and saved with the game.
- **Dev mode:** everything is written to disk.

Chat traces (prompt, reply, extracted facts and requests) and "your words → requests" traces use
the same format.

**Views:**
- **In game (dev mode):**
  - **Log tab:** the person page's Log tab lists decisions; clicking one opens the trace view.
  - **Trace view:** the prompt or features exactly as the brain saw them, every option with its
    score as bars, the rules' scores beside them, the choice highlighted, the reason, and the
    outcome.
  - **Pause on decision:** an option to pause on the next decision of the selected person.
- **For runs:** `colony-run traces <run>` opens a local trace explorer page (static HTML,
  generated). It can filter by person, brain, action or "brain disagreed with rules", and
  compare two runs.

## 8. Subagent bridge (Claude Haiku, no API)

1. **Batching:** in lockstep mode the sim gathers every decision due this tick into one batch,
   written to `runs/<id>/bridge/req-<n>.json`. Each item has the same text a text brain would
   see, the numbered options and an answer format: choice number plus a one-line reason.
2. **The sim pauses.** The UI shows "Haiku is deciding 14 choices…".
3. **A driver answers,** either way:
   - from a Claude Code session: I spawn Haiku subagents (about 20–30 decisions each, a few in
     parallel);
   - or with `tools/haiku_driver`, a script that calls the `claude` command-line tool in print
     mode with `--model haiku`, so long runs need no one in the loop.

   Both use the user's Claude plan, with no API key.
4. **Answers land** in `res-<n>.json`.
   - **Checking:** each answer is checked. A missing or invalid choice falls back to `rules`, and
     the trace says so.
   - **Resuming:** the sim resumes.
5. **Running out:** if the plan's usage limit is hit, the run saves and stops cleanly, and can
   resume later from the save.
6. **Chat:** conversations use the same bridge on the Max setting.

## 9. Learned brains and training

**The pipeline:** traces → datasets → training → evaluation → `brain/models/<name>@v<n>`.

- **Datasets:** `colony-run export` turns traces into datasets:
  - features with choice and outcome, for the scorer;
  - text with choice, for the text model and Laya;
  - dialogue, for a chat model later.

  Seeds are split into train and held-out sets.
- **Scorer:**
  - **Training:** tiny, so it trains on the Mac's CPU in seconds to minutes with Python scripts in
    `brain/`.
  - **Export:** weights are exported to a small file that Rust runs natively, with no runtime
    dependency.
- **Text model and Laya fine-tunes:** trained on Colab with the cheapest GPU (a T4), exported to
  ONNX (text model). No heavy training on the Mac.
- **Evaluation,** two ways:
  - **agreement:** match with the teacher on held-out seeds;
  - **colony outcomes:** survival, births, buildings, hobbies and beauty made, from arena runs.
- **The arena:** `colony-run arena --seeds 1..20 --brains rules,scorer@v1,subagent` runs the same
  worlds with different brains and writes a comparison report. The in-game Arena ledger shows it.
- **Laya:** kept only as an adapter and comparison, retrained now and then.
  - **v1 measurements:** about 380 ms per decision on the CPU with 3 threads, 79% agreement for
    v5. Quantising it didn't work: int8 broke agreement and ran slower.

## 10. Expansions (packs)

The base defines a `Pack` trait. Every expansion implements it. Packs are compiled in and switched
on per world when the world is created. Third-party mods (WASM) are a later option, not part of
the base.

**What a pack can add:**
- **Content:** data files (items, buildings, crops, animals, genes, needs, skills).
- **World generation stages.**
- **Systems** run every tick, hour, day or season, in a declared order.
- **Actions kinds,** with their features and text.
- **Tables:** per-person or per-tile data, keyed by id.
- **Event kinds,** with chronicle and story-feed wording.
- **UI:** inspector sections, lenses, ledger pages, god powers.
- **Save sections and trace fields.**

**The rule:** a pack changes the world only through the base's quantities and hooks. If an
expansion needs a hook that doesn't exist, the hook is added to the base on purpose and documented
here.

## 11. Renderer

- **Basics:** wgpu (Metal on the Mac) with winit, and an orthographic isometric camera.
- **Camera controls:** pan with drag, the keyboard and trackpad scroll; zoom with pinch and the
  wheel; it follows a selected person.
- **Models:** GLB files from the Blender pipeline (carried over from v1, `tools/blender/`).
  - **Batching:** one instanced draw per mesh part per variant (v1: hundreds of trees became a
    dozen draws), with per-instance colour for seasonal leaves.
  - **Animation:** skinned, with clip blending for walk, run, work and idle.
- **Terrain:** a mesh per chunk, rebuilt only when the chunk changes, with smooth normals. Ground
  colour comes from the tile fields (biome, footfall, moisture) in a tile texture.
  - **Biome edges:** colours blend across tile edges, dithered so they stay pixel art.
  - **Seasons:** they recolour grass and leaves, and snow settles in winter.
  - **Culling:** chunks and model groups are culled against the view and the sun.
- **Water:** a surface whose corners average the water around them, so rivers slope instead of
  stepping. It runs one tile in under the banks, so the waterline follows the ground rather than
  tile edges. Deeper water is darker, with ripples.
- **Lighting:** one sun with a shadow map, warm point lights (fires, windows) and a night grade.
- **Pixel pass** (v1's look, kept):
  - **Pixel size:** 24 art pixels per tile, belonging to the world. An art pixel is always a
    whole number of screen pixels.
  - **Camera snap:** the camera snaps to the art-pixel grid so panning doesn't crawl.
  - **Lines:** outlines where depth jumps, softer lines at creases.
  - **Resolution:** the scene is always drawn at art-pixel resolution, so a full-screen frame costs
    about the same as a small one. Measured on the M3 Pro: 2 to 5 ms GPU at 3024×1964.
- **The sharp overlay** is drawn after the pixel pass: text, icons, bubbles, selection rings and
  lens colours.
- **Targets:** 120+ fps full screen with 50 people on the M3 Pro, and no frame over 16 ms from
  the sim, brains or saves.

Art direction is in [STYLE.md](STYLE.md).

## 12. UI

- **Our own UI kit** (crate `ui`), immediate-mode, drawn by our renderer in the pixel style.
  - **Scaling:** it scales with the window in logical units.
  - **Text:** fontdue rasterises glyphs into one R8 atlas; shapes are rounded rects from a
    signed distance (`render/src/shaders/ui.wgsl`). Drawn last, at full resolution.
  - **Input:** clicks and scrolling go to the UI first; the world only gets them when the
    cursor isn't over a panel (`Ui::over_ui`, from the frame before).
  - **Pieces:** panels, tabs, rows, bars, hover cards, links, sparklines, small charts, a simple
    graph view and a tree view (family trees).
- **Layout:**
  - **Top bar:** settlement, era, date, speed and alerts.
  - **Story feed** on the left.
  - **Inspector** on the right, with tabs Mind, Life, Body, Skills, Log.
  - **God toolbar** along the bottom.
  - **Lens buttons** over the world.
  - **Ledgers:** full-screen pages for Society, Economy, Knowledge, History, Map and Arena.
- **Levels of detail:** the world, hover, the inspector, ledgers.
  - **Hover:** every number explains itself.
  - **One meaning per colour,** everywhere.
  - **Words before numbers.**
  - **One panel per side;** pin to keep a card.
  - **Detail setting:** minimal, normal or analyst.
- **Mockup first.** A clickable mockup with fake data was signed off at M0:
  [Colony UI mockup](https://claude.ai/artifact/6L5ZsA58ze41y5N6oMhmzK). It settled these:
  - **Floating panels:** dark, translucent panels float over the world instead of taking
    columns. The camera glides so the selected person sits in the clear middle.
  - **Two fonts:** a pixel font (Pixelify Sans) for titles, and a plain sans (Inter) for dense
    text and all numbers. (M4 changed numbers to Inter: Pixelify's digits misread, "15" looks
    like "18".) Pixelify is drawn at multiples of 8 window pixels so its pixels stay square.
  - **Event colours:** one colour per event kind, used in the feed, history and filters.
    - social: blue `#7FB8E6`
    - conflict: coral `#F0997B`
    - knowledge: purple `#C3A6F0`
    - world: green `#8CC56A`
    - life: gold `#E8C468`
    - your acts: pink `#F2A7C9`
  - **Need colours:** body needs are teal `#6CC4A1`, mind needs purple `#B49CF0`.
  - **Score bars:** in traces, the brain's scores are gold and the rules' second opinion is blue.
- **Lenses (M4):** traffic greys the world and shows footfall as heat in the ground shader;
  mood and needs put a coloured ring under each person (the selection ring, tinted).
- **Story feed (M4):** built from sim events, stamped with the tick they happened on
  (`Sim::take_timed_events`). The same thing happening to several people within three hours
  groups into one line ("3 people are very thirsty").
- **Headless UI shots:** `colony --shot out.png --inspect K --tab log --trace N --lens traffic
  --detail analyst --cursor X,Y` renders any view without a window; `--bench` takes the same
  options to time frames with the UI open.

## 13. Tools and tests

- **`colony-run`:** `run`, `arena`, `map`, `traces`, `export`, `bridge` and `bench` (sim speed).
- **`tools/shot.sh`:** screenshots at a given zoom, time and season, in a small background window
  on screen 0. **`tools/perf.sh`:** fps percentiles and the slowest frames.
- **Tests:**
  - unit tests;
  - determinism (same seed, same hash after N days);
  - save round-trip and branch;
  - survival (a rules colony lives a year);
  - generation sanity (rivers reach water, the start site is reachable).
- **Test windows** stay small, in the background, and few. Test runs never touch the player's saves.

## 14. Performance budgets (M3 Pro, normal power)

| What | Budget |
|---|---|
| Frame, full screen, 50 people | ≤ 8.3 ms (120 fps) |
| Sim tick, 50 people, live | ≤ 1 ms average, no tick over 4 ms |
| Headless, 50 people, rules brain | one game year (~35k ticks) in under 1 minute |
| Scorer decision | < 1 ms |
| Save | background, no frame hitch |

A lower-power preset (smaller window resolution, fewer shadows, Low AI) is added at the end of the
base; a real low-end target comes later.

## 15. Repository layout

```
Cargo.toml            workspace
crates/               sim, brain, trace, render, ui, colony, colony-run
data/base/            content (RON)
packs/                expansions (empty in the base)
assets/               models (GLB), fonts, icons
brain/                Python: training scripts, notebooks, the Laya adapter server
tools/                blender/, shot.sh, perf.sh, haiku_driver
docs/                 VISION, ARCHITECTURE, ROADMAP, LATER, STYLE
runs/                 headless run output (ignored)
archive/godot-v1/     the first build, runnable on its own
```

## 16. Lessons from v1 (built in from the start)

- **Batch drawing.** Batch everything drawn many times, and keep per-instance changes (tint,
  shrink, hide) cheap.
- **Pixel art that holds still.** Whole-pixel art blocks plus a camera snap: no shimmer when
  panning or zooming.
- **Spot searches.** Use summed-area tables, and cache them by map version.
- **Decision time limits.** Give decisions a per-frame time limit in live mode; never let one
  person's slow decision stall the frame.
- **Knowledge is per person:** places seen and heard, game sightings that go stale. Options
  offered must come from what the person believes, not from the truth.
- **Background work is always waited on** before quit, and saves are atomic.
- **Test runs never overwrite the player's autosave.**
- **Brain hardware.** Laya on the GPU caused frame hitches next to rendering; slow brains belong
  on the CPU or out of the live loop.
- **Sleep timing drifted into the day in v1.** Sleep and wake need a body clock, not just a
  rest need.
- **Rain particles made the view busy.** Show weather through light, ground and the top bar.
