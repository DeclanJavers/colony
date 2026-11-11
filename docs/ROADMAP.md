# Colony: roadmap to the base

The order of work for the base game ([VISION.md](VISION.md)). How it's built is in
[ARCHITECTURE.md](ARCHITECTURE.md); what comes after is in [LATER.md](LATER.md).

Each milestone ends with "done when" checks:
- **measured:** a test or number;
- **played:** you look and agree.

You sign off on a milestone before the next one starts. Milestones can be split if they grow.

---

## M0: Plan and mockup (done, signed off 2026-11-10)

Mockup: [Colony UI mockup](https://claude.ai/artifact/6L5ZsA58ze41y5N6oMhmzK) (private canvas).

- **Plan docs:** VISION, ARCHITECTURE, ROADMAP, LATER, STYLE.
- **Clickable HTML mockup with fake data:**
  - the main screen (top bar, story feed, world, inspector, god toolbar);
  - all five inspector tabs;
  - the dev trace view;
  - one lens;
  - two ledgers (History, Arena).

**Done when:**
- **played:** you've clicked through the mockup and the layout feels right;
- **played:** the docs match what you want.

## M1: The world from a seed (headless) (done, signed off 2026-11-10)

Generation takes about 30 ms per 256×256 region. Look at seeds with `colony-run maps --seeds 1..10`.

- **Workspace:** the Cargo workspace and crates, with pinned library versions.
- **Sim core:** tick, time and seasons, seeded random streams, tables and typed ids, events,
  commands, saves, the world-state hash.
- **World generation:** continents and sea, mountains and hills, rivers and lakes, climate,
  biomes, soil, plants and resource nodes, the start site.
- **`colony-run map --seed N`:** a PNG per layer.

**Done when:**
- **measured:** the same seed gives the same hash, twice and across a save and load;
- **measured:** every river reaches a lake or the sea, and the start site has water, food and
  wood in reach;
- **measured:** generating a 256×256 region takes under 2 s;
- **played:** across ~10 seeds, the maps look like real geography to you.

## M2: Seeing it (done, signed off 2026-11-10)

Full screen runs at 245 to 517 fps with no window at 3024×1964. Windowed, it holds the display's
144 Hz. Measure with `tools/perf.sh`.

- **App window:** winit + wgpu, with the camera (drag pan, trackpad pan and pinch, wheel zoom,
  keys).
- **World drawing:** terrain chunks, water, and trees, rocks and bushes from GLB models (Blender
  pipeline brought over).
- **Look:** sun, shadows and night, the pixel pass with snapping, seasons recolouring the ground
  and leaves.
- **Debug:** an FPS counter; `tools/shot.sh` and `tools/perf.sh`.

**Done when:**
- **measured:** full screen at 120+ fps on the M3 Pro, with no frame over 16 ms;
- **measured:** pan and zoom show no shimmer or crawl (pixel-change test as in v1);
- **played:** flying over a generated region looks like the style guide.

## M3: People who live (done, signed off 2026-11-10)

- **Survival:** a rules colony survives a year on 30 seeds.
- **Speed:** about 1 s per game year headless.
- **Body clock:** daytime sleep is under 1%.
- **Traces:** every decision leaves a full trace.
- **Fixes after your play test:**
  - shadows hold still, the camera no longer jitters, and zoom steps between fixed pixel levels;
  - the terrain mesh is smoother;
  - water has flow, plumes and foam, and river mouths open onto the sea;
  - walking matches speed, wading works, and sleep spots are on dry land.

- **People:** DNA and expression (height, build, skin, hair and eye colour, personality leanings),
  body, needs, mood with reasons, skills.
- **Knowledge:** places seen and heard, with source and trust.
- **Actions:**
  - survival: drink, fetch water, forage, eat, sleep with a body clock, keep warm;
  - materials and fire: wood, sticks, stone, fire;
  - checking the stores, exploring.
- **Decisions:** pathfinding; the brain plug with `rules`; live and lockstep modes.
- **Decision traces** from the first decision on (with the rules second opinion and outcomes).
- **Drawing people:** villager models with skinned animation (walk, run, work, idle, sleep), and
  what they carry shown in hand.
- **Basic UI:** a hover card and a plain inspector.

**Done when:**
- **measured:** a rules colony of 10 survives a game year headless on 10 seeds;
- **measured:** a year runs headless in under a minute;
- **measured:** no one sleeps through the day (body clock check);
- **played:** watching them for 10 minutes, what they do looks sensible and alive.

## M4: Understanding everything (done, signed off 2026-11-10)

- **The UI kit** in the pixel style, scaling with the window.
- **Main layout:** top bar, story feed (sentences, colour by kind, click to jump, grouping), god
  toolbar (empty slots for now).
- **Inspector:**
  - Mind: needs, mood and why, current thought, goals;
  - Body: DNA, which parent each trait came from;
  - Skills: skills and knowledge held;
  - Log: decisions;
  - Life: placeholder until M5.
- **Dev trace view:** what the brain saw (features and text), every option's score beside the
  rules' scores, the choice, the reason, the outcome, and "pause on this person's next decision".
- **Hover explanations** for every number.
- **Lenses:** traffic (footfall), mood, needs.
- **Detail setting:** minimal, normal, analyst.

**Done when:**
- **played:** you can click anyone and say what they're doing and why within a few seconds;
- **played:** the trace view shows exactly what the brain saw and picked for any decision;
- **measured:** still 120+ fps with the inspector and a lens open.

**Measured (2026-11-10, M3 Pro, 3024x1964 headless, inspector and a lens open):** zoom 16:
386 fps, zoom 40: 259 fps, zoom 120: 153 fps (the UI costs 0.3 to 0.7 ms). Windowed with the
inspector and traffic lens: 261 fps average.

## M5: Families and society

- **Social actions:** talk, share news, ask for help, teach, look after a child, courting.
- **Relationships and memory:** relationships that change with shared time, help, conflict and
  gossip; memory episodes that fade unless important.
- **Life:** partners, pregnancy, birth with inheritance, childhood, ageing, death, graves.
- **Knowledge in people:** skills and recipes are taught, and lost when no one living knows them.
- **The chronicle:** events written as history, with a timeline.
- **Life tab:** family tree, relationships, memories.
- **History ledger.**

**Done when:**
- **measured:** over 5 headless generations, children's traits match inheritance (checked
  statistically against parents);
- **measured:** at least one seed loses a skill when its last holder dies, and the chronicle
  records it;
- **played:** reading one family's history in the game tells a real story.

## M6: Settling down

- **Building:** shelter, house, storage, fire pit. Homes and families living in them.
- **Basic farming:** one crop, plant, tend and harvest, with growth from soil moisture,
  fertility, temperature and season. Rain and season drive moisture.
- **Desire paths:** footfall wears grass into trails that are faster to walk.
- **Weather:** temperature, rain and snow change the tile fields and needs.
- **Animals:** deer and rabbits with populations (birth, death, fear, migration from the edges),
  and simple hunting with a spear.
- **Lens:** soil.

**Done when:**
- **measured:** a rules colony starts farming on its own in most seeds by the end of year 2;
- **measured:** on the traffic lens, trails form between home, water and work, and trip times
  fall over time (shown as a stat);
- **measured:** overhunting a small region visibly drops the deer population, and it recovers
  when hunting stops;
- **played:** the settlement looks lived in and grows in a way that makes sense.

## M7: Leisure, hobbies and beauty

- **Free time:** rest, walk, play, and hobbies (garden, carve, sing), drawn by personality.
- **Hobbies:** a hobby emerges from repeated enjoyment, appears on the person page and grows a
  skill.
- **Beauty:** plant flowers, carve statues, decorate homes, lay stones. These persist in the
  world and add to the beauty field.
- **The "beauty matters" switch** (in world settings) and a beauty lens.

**Done when:**
- **measured:** with the switch on, mood near beautiful things is measurably higher;
- **measured:** with it off, beauty has no effect on any number;
- **played:** people have hobbies you can name, and the village has some beauty in it.

## M8: Brains I (subagents, chat, AI budget)

- **The subagent bridge:**
  - batches decisions and pauses the sim, with a "Haiku is deciding…" indicator;
  - two drivers: a Claude Code session with Haiku subagents, and `tools/haiku_driver` using
    `claude -p --model haiku`;
  - answers are checked, with a fallback to rules;
  - clean stop and resume when usage limits are hit.
- **Chat:**
  - code templates;
  - a local LLM through llama.cpp, with grounded facts and requests pulled out of
    conversations;
  - the subagent on the Max setting.
- **Talking to people** (god tool: speak): your words become requests, by keyword matching or an
  LLM, and the trace shows how they were read.
- **Thoughts** on the person page, by template or LLM.
- **The AI budget setting** with presets picked from the hardware.
- **Experiment B:** a subagent colony-day. Measure real time, usage per game day, and how its
  colony differs from rules.

**Done when:**
- **measured:** a subagent colony runs 3 game days unattended with `haiku_driver`, and its traces
  have a choice and reason for every decision;
- **measured:** on Low, the game runs with no servers;
- **played:** talking to someone changes what they do, and the trace shows why;
- **played:** subagent colonists feel different from rules colonists in ways you can see.

## M9: Brains II (distilled models and the arena)

- **`colony-run export`:** datasets from traces, with held-out seeds.
- **Experiment A, the scorer:** trained on subagent traces, run natively. Compare agreement, speed
  and colony outcomes against rules and Laya.
- **The text model:** distilled into a small encoder on a Colab T4, run through ONNX in Rust.
- **The Laya adapter** and one fine-tune on the new traces, for comparison.
- **The arena:** same seeds, different brains, a comparison report, and the Arena ledger in game.
  The report includes the beauty question: does each brain make beauty with the switch off?
- **Branching saves:** fork a world from any save, from the UI and the CLI.

**Done when:**
- **measured:** the scorer is under 1 ms per decision;
- **measured:** the scorer's colonies do about as well as the subagent's in outcomes (target set
  after experiment B);
- **measured:** the text model fits the live time limit for named people;
- **played:** the Arena ledger makes the differences between brains clear at a glance.

## M10: Finishing the base

- **The other ledgers:** Society, Economy (stock only), Knowledge, Map.
- **God tool: gift** (food, materials, a building).
- **Polish:** settings, main menu, new world (seed, size, packs: none yet, beauty switch),
  save slots, a lower-power preset.
- **Audio** basics (optional here, or later).
- **The pack interface,** proven by moving one small system (e.g. hunting) behind it without
  changing behaviour.
- **A blind playthrough** by a fresh agent, plus your own.

**Done when:**
- **played:** you'd happily play the base for an evening;
- **measured:** all tests green, the performance budgets met, and hunting works the same as a
  pack;
- **played:** you sign off the base, and the first expansion is chosen from LATER.md.
