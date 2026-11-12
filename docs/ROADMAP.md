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

## M5: Families and society (done, signed off 2026-11-11)

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

**Measured (2026-11-11, after the people pass: 48 art pixels per tile, places by the fire):**
- **Inheritance** (`crates/sim/tests/families.rs`, seed 2): 5 generations and 60 children.
  - Every allele came from a parent (0 mutations seen of about 3 expected).
  - Heterozygous parents passed on each allele 49.9% of the time over 1,410 (z = -0.11).
  - Genetic height: slope 1.05 ± 0.21 on the parents' average (1 expected).
  - Height: slope 0.55 ± 0.18 (h² = 0.81 expected, within 3 standard errors).
- **Lost skills:** stonework was lost on all 6 seeds tried, each time with a chronicle entry.
  Nobody bothers to teach it while stone has no use. Firemaking was never lost.
- **Colonies:** 10 founders become 18 to 40 people by year 16, with 3 to 4 generations.
  Almost every death is old age (over 6 seeds: one starvation, one childbirth). One seed
  shrinks for a while when couples are few.
- **Speed:** about 1.7 s per game year headless at 25 to 35 people.
- **Frame rate:** 169 to 423 fps at 3024×1964 with 25 people, the Life tab and a lens open.

## M6: Settling down (done, signed off 2026-11-11)

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

**Measured (2026-11-11, `crates/sim/tests/settling.rs`):**
- **Farming:** all 8 seeds tried sowed their first field by day 6 (most on day 1 or 2, once the
  first shelter stood) and had harvested 353 to 534 grain by the end of year 2.
  - People found the seed themselves: wild grain in meadows. Founders know farming.
  - One harvest a year per field: a spring sowing ripens through summer and is cut in autumn.
- **Trails and trip times** (seeds 1 to 3, first season against year 3):
  - Trails save 5–6% of walking time at first and 27–33% by year 3.
  - Pace falls from 251–271 to 207–215 minutes per 100 m.
  - 93 to 143 tiles are worn at least half way to trails.
  - The traffic lens shows the stat.
- **Overhunting** (seed 2): three quarters of the deer were killed in one go, the way hunts kill
  them (scaring the ground). That took 17 deer to 5. They came back to 11, 12, 12 and 14 over
  the next 4 years; the land holds 16.
  - People also hunt on their own: 4 to 6 kills per colony in 6 years. That's mostly deer:
    rabbits give little meat.
- **Colonies** (`colony-run life --seeds 1..4 --years 6`):
  - 10 founders become 16 to 29 people by year 6, in 4 to 8 houses.
  - They harvest 1,500 to 2,200 grain in all.
  - Deaths over 4 seeds: 3 of old age, 3 of cold, 2 of starvation.
  - Food runs low at the end of each winter (0 to 3 days left).
- **Frame rate:** 385 fps at zoom 16, 196 at 40 and 171 at 120 at 3024×1964 (seed 2, year 3,
  with buildings, fields and animals).

## M7: Leisure, hobbies and beauty (done, signed off 2026-11-11)

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

**Measured (2026-11-11, `crates/sim/tests/beauty.rs`):**
- **Beauty on vs off** (seeds 3 and 4, a year of samples after two years of making things):
  - Somewhere beautiful (the same field in both), mood is 5.64 ± 0.18 with beauty mattering and
    3.13 ± 0.18 without.
  - That's 383 and 325 samples of awake grown people.
- **Beauty off changes nothing:** with the switch off, a world where the beauty field is
  scrambled with noise every tick ends with the same state hash as one where it isn't. That's
  40 days and about 700 free-time activities.
- **Hobbies:** after 4 years (seeds 1 to 3), 25 of 36 grown people have one.
  - They're of 4 kinds: walking, singing, gardening and carving.
  - People made 62 things: flower beds, statues and steps of decorating a home.
- **Colonies** (`colony-run life --seeds 1..4 --years 6`):
  - 21 to 31 people by year 6, up from 16 to 29 in M6. That's partly a fix: babies now sleep
    under their carer's roof.
  - 10 to 18 flower beds, 3 to 7 statues and 7 to 15 steps of decorating per colony.
  - Deaths over 4 seeds: 1 of old age, 1 of cold.
- **Frame rate:** 359 fps at zoom 16 at 3024×1964 (seed 3, year 3).

## M8: Brains I (subagents, chat, AI budget) (done, signed off 2026-11-11)

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

**Measured (2026-11-11):**
- **A subagent colony runs 3 game days unattended** (`colony-run bridge --seed 3 --days 3` with
  `tools/haiku_driver`).
  - **Calls:** 131 batches of up to 24 decisions, about 8 s each, with no failures. That's 394
    decisions, and 20 minutes of real time.
  - **Traces:** all 384 closed decision traces have a choice and a reason, and none fell back to
    rules.
  - **The colony:** all 10 people alive after 3 days.
- **Experiment B, Haiku colonies against rules ones** (seed 3, 3 days):
  - **Agreement:** Haiku picks what the rules brain would have only 8% of the time (31 of 384).
  - **What Haiku does:** it looks after needs first. 53% of its choices are eating, drinking or
    sleeping, 20% food and water for the camp, 15% the fire, and 9% free time.
  - **What it doesn't:** it built nothing and explored 9 times. The same seed under rules made
    56 building choices and explored 63 times. The bridge's instructions put urgent needs
    first, which may be why.
  - **Cost:** about 44 Haiku calls per game day with 10 people.
- **On Low the game runs with no servers:** Low is the rules brain, and chat and speaking are
  code (templates and keywords). Every test runs that way.
- **Speaking changes what people do** (`crates/sim/tests/speak.rs`): asked for wood, 12 of 16
  people fetched it within their next 3 decisions, against 1 of 16 unasked. The trace shows the
  request on the option and in the text.
- **Not done (needs you):** the local LLM (llama.cpp needs a model download), so chat and
  speaking use only their code rungs. Medium and High wait for M9's models.

## M9: Brains II (distilled models and the arena) (partly done; the rest after M10)

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

**So far (2026-11-11):**
- **The scorer** (`crates/brains/src/scorer.rs`) is trained in Rust (section 9 of ARCHITECTURE.md
  says why).
  - **Speed:** about 140 µs per decision.
  - **v1** learned from rules colonies only (`colony-run train`). It agreed with rules 72% of
    the time on held-out seeds, but only 54% in play: its own mistakes took it into situations
    the training data never showed, and it harvested a third of the grain.
  - **v3** is distilled by `colony-run distill`. Round 0 learns from rules colonies (seeds 1 to
    12, 48 days); in each later round the scorer runs the colonies itself and the rules brain
    says what it would have done in every situation the scorer got into. The model also gained
    per-kind option weights, plain minutes, and "knee" versions of amounts (food_days and the
    like), which is how the rules weigh them. Round 1's model (`scorer@rules-v3-r1`) plays best.
- **The arena** (`colony-run arena`), on 30 seeds no brain trained on (43 to 72), 48 days:

  | Brain | Alive | Died of hunger, thirst or cold | Grain | Things made | Hobbies | Agrees with rules |
  |---|---|---|---|---|---|---|
  | rules | 17.1 | 0.3 | 495 | 19.4 | 8.3 | 100% |
  | scorer@rules-v1 | 17.0 | 0.7 | 175 | 13.2 | 3.7 | 54% |
  | scorer@rules-v3-r1 | 17.0 | 0.0 | 423 | 15.4 | 7.0 | 74% |

  v3 keeps colonies alive as well as rules, and gets 85% of the grain, 79% of the things made
  and 84% of the hobbies. The report also lists where each brain parts from rules most; for v3
  the biggest is resting or talking when rules would sing (about 1% of decisions each).
- **Branching:** `colony-run fork` makes a branch whose chance events go their own way.
- **Export:** `colony-run export` writes `features.jsonl` and `text.jsonl`, each row marked train
  or held-out by seed. 512 Haiku decisions are in `brain/data/haiku`.
- **The Arena ledger** in game (developer view: the Arena button) shows the last report as bars,
  and where each brain parts from rules.
- **The beauty question** (`--beauty-off`, seeds 43 to 52): with beauty not mattering, rules
  colonies still make 20 things each and v3 colonies 16. Both make them for the fun of it.
- **Haiku as the teacher (after M10):**
  - **Instructions fixed:** told what the rules brain sees as numbers (a roof or not, cold
    nights, winter coming, materials ready), Haiku colonists made 26 building choices on the first
    day of seed 3, against none before.
  - **The dataset** (`brain/data/haiku-v2`): 6,840 decisions labelled by Haiku with
    `colony-run label`, 285 calls in all. 5,040 come from rules colonies over two years, and
    1,800 from a correction round (the situations a Haiku-trained scorer got itself into).
    Haiku picks what rules would only 9 to 16% of the time.
  - **A scorer trained on Haiku,** 20 unseen seeds, 48 days:

    | Brain | Alive | Died of hunger, thirst or cold | Homes | Grain | Born |
    |---|---|---|---|---|---|
    | rules | 16.9 | 0.3 | 5.9 | 481 | 7.2 |
    | scorer@haiku-v1 (rules situations only) | 0.0 | 10.0 | 0.5 | 0 | 0 |
    | scorer@haiku-v2 (with the correction round) | 9.9 | 0.1 | 8.0 | 0 | 0 |

    v1 copied Haiku's habit of eating whenever a little peckish and starved. With the
    correction round, colonies survive and build but never farm or have children.
- **Farming and families relabelled** (`brain/data/haiku-v3`): the text now tells Haiku
  about partners, children, fields and seed grain, and every decision that offered farming or
  family was relabelled (112 calls). A scorer trained on v3 keeps 19.7 alive after two years
  with 10 born (v2: none born), but still farms nothing.
- **The text model** (`brain/text_model/`, run in Rust through ONNX Runtime, `text-NAME` in the
  arena). Trained on Colab's T4 in about 10 minutes.
  - **Agreement with Haiku** on held-out decisions: 61% (the scorer on the same data: 52.5%).
    The game reads it exactly as training did (60.8% through Rust).
  - **Speed:** about 50 ms a decision on 4 CPU threads with option lines cached (15 ms with
    situations cut short, which costs most of its accuracy). Fine for a few named people, with
    decisions answered off the main thread; too slow for a whole colony at 10x.
  - **v1** (trained on v3) starved its first arena colony within 24 days. **v2** adds a
    correction round (`brain/data/haiku-v4`: 1,800 decisions from v1's own colonies, 75 calls).
    Seeds 43 to 47, 24 days:

    | Brain | Alive | Born | Died of hunger, thirst or cold | Homes | Grain | Things made |
    |---|---|---|---|---|---|---|
    | rules | 15.6 | 5.8 | 0.0 | 5.6 | 204 | 13.6 |
    | text_model_v2 | 17.0 | 7.8 | 0.8 | 6.6 | 141 | 0 |

    The first Haiku-taught brain that farms (three of five colonies harvested). It takes no free
    time at all: nothing made, no hobbies.
- **Haiku calls in all:** 513 (41 testing the instructions, 210 and 75 for the first dataset,
  112 relabelling farming and family, 75 for the text model's correction round).
- **Laya is back** (`tools/laya_driver` on the bridge's files; 0.27 s per decision). Seed 13,
  6 days: base Laya starves everyone (it has never seen this game's words), and v1's fine-tune
  (v5) keeps 9 of 10 alive with 8 homes, though it chooses sleep far too often.
- **Still to do, after M10** (you chose on 2026-11-11 to finish the base first):
  - ~~the text model~~ (done: text_model_v2);
  - ~~the Laya adapter~~ (done: `tools/laya_driver`); its fine-tune on the new traces;
  - the text model in the game (the AI budget's Medium setting), on a thread of its own;
  - a scorer trained on Haiku, compared in the arena with Haiku colonies (it needs more Haiku
    decisions, on your plan). Until then the "about as well as the subagent's" check is measured
    against rules instead.

## M10: Finishing the base (done, signed off 2026-11-11)

Started 2026-11-11, before M9's last parts (the base comes first).

- **The other ledgers:** Society, Economy (stock only), Knowledge, Map.
- **God tool: gift** (food, materials, a building).
- **Polish:** settings, main menu, new world (seed, size, packs: none yet, beauty switch),
  save slots, a lower-power preset.
- ~~**Audio** basics (optional here, or later).~~ Moved to LATER.md (your call, 2026-11-11).
- **The pack interface,** proven by moving one small system (e.g. hunting) behind it without
  changing behaviour.
- **A blind playthrough** by a fresh agent, plus your own.

**Done when:**
- **played:** you'd happily play the base for an evening;
- **measured:** all tests green, the performance budgets met, and hunting works the same as a
  pack;
- **played:** you sign off the base, and the first expansion is chosen from LATER.md.

**So far (2026-11-11):**
- **Gift** (the god toolbar): food and materials into the stores, or a finished shelter, house
  or storehouse near the camp (`crates/sim/tests/gifts.rs`).
- **The ledgers:** Society, Economy, Knowledge and Map, beside History and Arena under one
  Ledgers button.
- **Menus and saves:** main and pause menus, a new world (seed, size, beauty, packs), five save
  slots and an autosave, and settings (the lower-power preset, text size, full screen). Tested
  in the real window by driving it (`colony --drive`).
- **The pack interface:** hunting moved behind it with no change at all. The behaviour digest
  for seeds 1 to 3 over 48 days (with 2, 5 and 3 animals hunted) is identical before and after,
  and so are the arena's numbers for every seed (43 to 47, rules and scorer).
- **Also:** song bubbles draw a music note (the fonts have none); the save format is 6.
- **Blind playthrough 1** (`docs/playtests/blind-01.md`): a fresh agent played two seasons
  through the real window. Its verdict: "worth an evening to watch, not yet to play". Fixed
  since:
  - losing the world on a load (it's kept as "Your previous world");
  - god tools waiting for time to run;
  - speaking with no reply or sign it worked;
  - no way to find someone (names now move the camera; Follow; the lenses list who's low);
  - developer surfaces shown to players;
  - the bottom bar under the inspector, and the wording slips.

  Its "clicks land at 1.2x" turned out to be its own coordinate conversion from scaled
  screenshots, so the driver's shots now come at point size. Its screenshots dying was real:
  a covered window gave no image, and the UI lost its new glyphs; both are fixed.
- **Signed off** 2026-11-11. Audio moved to LATER.md; a second blind playtest is optional.
