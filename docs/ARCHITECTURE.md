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

**Families and society (M5).** The numbers live in `data/base/life.ron`.
- **Life pace:** people age 4 years per game year (one per season), so a child grows up in
  about 80 minutes at 10× and a life lasts about 5 hours.
  - **Babies** (under 3): make no decisions. They stay at their carer's side (their mother if
    she's alive, else their father, else the closest grown relative).
    - Under 2 they nurse from their mother.
    - After that they share their carer's meals and drinks, and anyone can bring them food and
      water from the stores.
  - **Children** (3 to 13): a short list of options: eat, drink, sleep, play, talk, and learn by
    watching an adult.
  - **Adults** (14 and up) and **elders** (55 and up): elders' health slowly gets weaker, and
    the chance of dying of old age rises each year.
- **Family:** mother, father, partner and children are stored on each person. Kinship (parent,
  child, sibling, grandparent) is worked out from those links.
- **Two-person choices use invitations.** Asking someone to be partners, to try for a child, to
  teach them, or for food puts an invitation in the other person's inbox. It shows up as an
  option at their next decision, so their own brain accepts or lets it lapse, and that is traced.
  Unanswered invitations lapse after 6 game hours.
- **Social actions:**
  - **Talk:** to someone awake nearby. It eases social for both, raises familiarity, and moves
    affection by how well their traits fit. A clash can turn into an argument. They pass on news
    (places the other doesn't know about) and gossip (opinions of a third person).
  - **Court:** raises romance.
  - **Ask to be partners** and **try for a child:** both by invitation.
  - **Feed a baby or child.**
  - **Teach** (by invitation) and **learn by watching.** Teaching weighs how useful the skill
    is right now: what its work is worth, and fire before cold nights. So a skill whose products
    nobody needs (stonework, until tools and building want stone) isn't passed on, and dies out.
  - **Ask for food:** when someone has none.
  - **Bury the dead:** a body lies where the person died until someone buries it, and a grave
    marks the spot.
- **Relationships:** for each pair: familiarity, affection, respect and romance (each 0 to 1,
  affection -1 to 1), plus whether they are partners. Familiarity grows with time spent
  together; the other measures change through talk, help, teaching, arguments and gossip. All of
  them slowly fade if unused.
- **Memories:** episodes with a feeling (-1 to 1) and an importance (0 to 1). Their strength
  fades by the day, slowly for important ones, and weak memories are forgotten. Recent strong
  memories count towards mood ("Grieving Bram").
- **Pregnancy and birth:** a child is conceived only after both partners accept "try for a
  child". Pregnancy lasts 9 months of age-time (about 4.5 game days). The child takes one allele
  per gene from each parent and gets a name nobody alive has. Each person records their
  generation.
- **Skills that can be lost:** skills marked `taught` (firemaking, stonework) are known only at
  level 1 or more.
  - Only some founders know them.
  - Without one, its actions are impossible ("doesn't know how to make fire").
  - They can only be learned from someone who knows them.
  - When the last person who knows one dies, a `SkillLost` event and a chronicle entry record it.
- **The chronicle:** `Sim::chronicle` keeps history written from events: births, deaths,
  partnerships, teachings, skills lost, firsts, and each year's summary. Each entry has the
  people involved and an importance. The History ledger shows it as a timeline, filtered by the
  detail setting. It is saved.
- **As built (M5):**
  - **Other people:** actions read a snapshot of everyone taken at the start of the tick
    (`family::Seen`). Changes to anyone but the person acting are `social::Effect`s, applied in
    order after everyone's turn, so results never depend on who moved first.
  - **Inviting someone** interrupts what they're doing only if it's easy-going: resting,
    playing, exploring, watching, talking.
  - **Arguments** scale with temper (low calm on both sides), bad feeling, and strain (hunger,
    tiredness, cold).
  - **Affection** grows more slowly the higher it already is.
  - **Ageing in the UI:** spans of time past a season are shown in years of age, so they match
    people's ages.
  - **Places by the fire:** each person has their own spot, for sitting and for sleeping. Spots
    are on rings inside the fire's warmth, and the youngest get the closest. Before this, people
    stacked on the same tiles. Unstacking first pushed the youngest out of the fire's reach and
    they froze.
  - **Burial ground:** someone who dies near the fire is buried at least 8 m out, on the side
    where they lay. Graves among the living would take their places by the fire.
  - **Exploring in the evening** scores lower, so people don't end up far from the fire at
    bedtime.
  - **Headless check:** `colony-run life --seeds 1..6 --years 16 [--chronicle]` follows colonies
    over the years.

**Settling down (M6).** The numbers live in `data/base/buildings.ron`, `farming.ron`,
`weather.ron` and `animals.ron`.
- **Weather:** each day rolls a weather (clear, cloudy, rain, snow; snow only below freezing) from
  the season and the region's rain.
  - It shifts the day's temperature: clear nights are colder, cold snaps come in winter.
  - Rain raises soil moisture, and dry warm days dry it out.
  - Snow builds a snow cover that melts on warm days. The renderer draws that cover, instead of
    snow following the calendar.
  - Being out in rain or snow chills people, unless they're under a roof or by a fire.
- **Buildings:** a shelter (sleeps 2), a house (a family), and a storehouse (stops food spoiling).
  The fire pit is the camp's fire.
  - **Building is a choice like any other.** Someone with no home, or the group short of
    storage, can "start a house"; the sim picks a clear spot near the camp and makes a site.
    Anyone can then work on it: they fetch the materials from the stores and build.
  - **Homes:** a finished home belongs to the household that started it (a couple and their
    young children). They can sleep there: a roof cuts the night's chill, so a family can get
    through a winter night without the fire. Whoever sleeps at home is inside, so isn't drawn.
    - Families build houses, people on their own a shelter. Grown children live in their
      parents' house until they have a family of their own, then build one.
    - **Homes change hands** once a day (`settle::rehouse`): a dead owner's home goes to their
      partner or a child; a household that moved into a house lets its shelter go; an empty
      home goes to an adult who has none, families first. An empty shelter falls down after 4
      days, a house after 24.
  - **Claims:** two people never start the same kind of building at once, or sow, tend or
    harvest the same field (each sees what the others are doing in the tick's snapshot).
  - **Order:** people settle before they farm. The first field waits for the first finished home.
- **Farming (one crop, grain):**
  - **Seed:** wild grain grows in meadows. Gathering it gives grain, and grain is also seed.
  - **The field cycle:** clear a field (a 3×3 patch near the camp), sow it with seed in spring or
    summer, tend it, and harvest it when ripe.
  - **Growth** each day comes from the soil's moisture, fertility and temperature, and the season.
    Tending keeps weeds down.
  - **Frost** (the night's low, below -3 C) scorches unripe grain; 4 degrees lower kills it.
  - **Harvest:** a ripe field gives up to 120 grain (less for poor health and unskilled hands).
    From autumn, a field 60% grown can be cut early for that share, rather than lost to the
    frost. In practice a spring sowing is cut in autumn: one harvest a year per field.
  - **Stored food spoils:** berries quickly, grain slowly, meat fastest; a storehouse slows all
    of it.
  - **What stone is worth** to people rises while the camp needs it: for the storehouse, and for
    spear points while there are fewer than two spears.
- **Desire paths:** footfall already makes trails cheaper to route and faster to walk. Rough ground
  (marsh, forest, hills) is slower off the trails.
  - **The trip-time stat** (`Stats::walk`, per season): minutes of walking per 100 m, and the
    same walks' minutes over unworn ground. Their ratio is the share of walking time the trails
    save, which takes out the effect of where people happen to walk.
- **Animals:** deer and rabbits live in herds, wander their range, graze, and flee people.
  - **Population:** they give birth in spring, die of age and of winter crowding, and new
    animals arrive from the map's edges while the population is below what the land holds.
  - **Hunting:** with a spear, made from wood and stone by someone who knows stonework. This
    gives stone, and so stonework, a use. Hunting yields meat, which is filling but spoils fast.
  - **Fear:** hunted animals become warier, and a hunted area gets a "fear" that fades. Herds move
    their range off frightened ground; calm herds' ranges drift over good ground.
  - **Knowing where game is:** people remember a sighting for three days (herds keep to a
    range), and a hunt follows the animal from there. A stale sighting lowers the expected
    catch.
- **Lens:** soil, showing fertility (stepped from poor to rich) with wetter ground darker.

**Leisure, hobbies and beauty (M7).** The numbers live in `data/base/leisure.ron`.
- **Free time** is choices like any other, picked when needs are met. Rest and play are
  there already. M7 adds these:
  - **stroll:** a walk to a pleasant spot and back. Curious people like it.
  - **sing:** by the fire with others. Sociable people like it.
  - **garden:** plant a flower bed by home, in spring or summer. Warm, steady people like it.
  - **carve:** whittle a piece of wood. Curious, calm people like it. With skill, the carving is
    a statue, set up near home or the fire.
  - **decorate:** improve your home, one step at a time: stones laid to the door, flower boxes,
    then carved posts. Diligent people like it.
  - Each one eases fun (and singing eases social too). A person's personality makes each one
    more or less appealing, so different people spend their free time differently.
- **Hobbies emerge.** Doing a free-time activity adds to how much the person has enjoyed it:
  more when it suits their personality, and more when they needed the fun.
  - Past a threshold it becomes their hobby (two at most). It then appeals more and is more
    fun, and it's shown on the person page and written in the chronicle.
  - Gardening, carving and singing are skills that grow with practice. Better carvers make
    statues that are more beautiful.
- **Beauty** is a field over the map (`Sim::beauty`), from what people made and what grows:
  - **Sources:** wild flowers, flower beds (both much less in winter), statues (more for a
    skilled carver), and decorated homes (more for each step).
  - **The field** is rebuilt whenever its sources change. It isn't saved: it's worked out again
    from the saved world.
  - **Flower beds fade** unless someone tends them; gardening tends a wilting bed before
    planting a new one.
  - **Everyday pastimes** (a stroll, a song) take more enjoyment to become a hobby than making
    things does, so hobbies vary.
  - **Free time waits** while the stores hold under a day and a half of food, or while the
    person is hungry, thirsty or cold themselves.
- **The "beauty matters" switch** (a world setting, saved, on by default; changed by a command
  like any god act).
  - **On:** being somewhere beautiful lifts mood ("Lovely here") and slowly eases boredom.
    Strolls head for beautiful places.
  - **Off:** nothing reads the field: not mood, needs, choices, or any other number. The beauty
    lens still shows it. People still have hobbies and still make things, for the fun of it.
- **Beauty lens:** the field as a warm glow on the ground, with what made it.

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

**As built (M8):**
- **The bridge brain** (`crates/brains`, `SubagentBrain`) is a brain like any other, plugged in
  beside rules.
  - **Batches:** it collects decisions for 15 game minutes, or until 24 are waiting, then writes
    them as one batch. The people waiting stand and think meanwhile.
  - **Flushing:** when everyone who decides is waiting, the batch goes out at once.
- **Pausing:** while a batch is out, time waits. `Sim::poll_brains` takes answers without moving
  time on.
  - **Headless:** `colony-run bridge` waits in lockstep.
  - **The game:** on Max it pauses, with "Haiku is deciding N choices..." under the top bar.
- **Checking:** an answer must name one of the numbered options (the possible ones, as the text
  lists them). Anything else falls back to the rules brain, and the trace says so. Answers may
  come wrapped in words or a code fence.
- **`tools/haiku_driver`** sends each batch to `claude -p --model haiku` with every tool turned
  off (it can only read and answer).
  - **Retries:** three tries, then the batch is left to the rules brain.
  - **Usage limits:** a limit message writes `stop.json`. The run saves, stops and prints how to
    resume (`colony-run bridge --resume`).
  - **In the game:** the game starts and stops the driver itself on Max.
- **Speaking (god tool):** the person panel has "Speak to ...". On Low the words are read by
  keyword (`data/base/speak.ron`) into the actions they ask for.
  - **What it does:** the request lasts 6 game hours, and hearing it interrupts anything but
    sleep or an urgent need. Options it asks for carry an `asked` feature, which the rules brain
    likes (+1.0). The text a text brain sees opens with what they heard.
  - **Ending:** doing one of the things asked ends the request ("The voice asked me to fetch
    wood. I'll do it.").
- **The AI budget** is in World settings. Low (rules) and Max (Haiku) work now. Medium and High
  need M9's models and a local LLM; until then they act as Low.
- **Chat on Low** is code templates (`sim::chat`). Starting to talk, someone says a line picked
  from what's on their mind:
  - food running short, hunger, cold, rain or snow, the fire out, being tired or lonely;
  - their hobby, a lovely spot (only when beauty matters), their partner, the other's child;
  - otherwise a greeting.
  Singing by the fire gives a line of a song. Lines show as speech bubbles: a fixed size on
  screen, and never overlapping (the one nearer the camera wins). They don't change any number.

## 9. Learned brains and training

**The pipeline:** traces → datasets → training → evaluation → `brain/models/<name>@v<n>`.

- **Datasets:** `colony-run export` turns traces into datasets:
  - features with choice and outcome, for the scorer;
  - text with choice, for the text model and Laya;
  - dialogue, for a chat model later.

  Seeds are split into train and held-out sets.
- **Scorer:**
  - **Training:** tiny, so it trains on the Mac's CPU in seconds. (Changed at M9: it trains in
    Rust, `colony-run train`, instead of Python scripts in `brain/`. That needs no packages
    installed, and the same code that trains it runs it.)
  - **The model:** a linear score per option over the option's own features, plus, per kind
    of action, a bias, each situation feature (so "thirsty" can pull towards drinking and away
    from chopping) and each option feature again (a field's weeds matter to tending, not to
    sowing). Derived features carry the shapes the rules use: relief × urgency, gain × value,
    option features × boredom and × sleepiness, plain minutes, and hinge versions of amounts
    (1 − food_days/k for k = 1.5, 3, 6). It's trained by softmax over each decision's options.
  - **Distilling** (`colony-run distill`, changed at M9): learning only from the teacher's own
    colonies fails in play, because the student's mistakes lead into situations the teacher
    never met (v1: 72% agreement offline, 54% in play). So after round 0 the scorer runs the
    colonies itself, the rules brain labels every decision it made with what rules would have
    chosen, and it trains again on everything (DAgger). Each round's model is saved, and the
    arena on unseen seeds picks the one to use.
  - **Export:** weights in a small JSON file (`brain/models/<name>.json`) that Rust runs
    natively, with no runtime dependency.
- **Text model and Laya fine-tunes:** trained on Colab with the cheapest GPU (a T4), exported to
  ONNX (text model). No heavy training on the Mac.
- **Evaluation,** two ways:
  - **agreement:** match with the teacher on held-out seeds;
  - **colony outcomes:** survival, births, buildings, hobbies and beauty made, from arena runs.
- **The arena:** `colony-run arena --seeds 43..72 --brains rules,scorer@rules-v3-r1` runs the same
  worlds with different brains and writes a comparison report, with where each brain parts
  from rules most. The in-game Arena ledger shows it. Arena seeds are never training seeds.
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

**Built at M10** (`crates/sim/src/pack.rs`):
- **The `Pack` trait:** its id and name, the acts it adds, and hooks for the options a grown
  person is offered (with their features), stepping its actions, the rules brain's weight for
  them, the thought behind choosing one, whether children may, and whether doing it scares
  animals.
- **Compiled in:** `PACKS` lists them; the order is saved in worlds. A pack's action is
  `Kind::Pack { pack, act, target }`, and its name is the act's id, so brains, traces and the
  scorer see "hunt" as before.
- **Per world:** `Settings::packs` (saved), chosen in the new-world menu. All of the base's packs
  are on by default.
- **Helpers:** packs build actions from `pack::api` (walking, taking from the stores,
  practising skills, effects on others), the same helpers the base's own actions use.
- **The first pack is hunting** (`packs/hunting.rs`): making spears and hunting with them. The
  animals stay in the base: wildlife is a quantity of the world, and hunting is a way to change
  it. Moving it changed nothing: a digest of every event and each morning's state is identical
  before and after (`crates/sim/tests/packs.rs`), and so is the arena.
- **Not hooks yet** (hunting uses the base's for now): a pack's own content files, event kinds,
  per-person tables, UI pages and save sections.

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
- **Buildings, fields and animals (M6)** are drawn fresh each frame from the sim (`ThingDraw`,
  `AnimalDraw`), as dynamic instances.
  - **Model flags:** a model's `window` parts glow when the instance is lit (someone lives
    there, in the evening and before dawn); `roof_*` materials (except the dark ridges) take
    snow in patches as the cover builds.
  - **Fields** are drawn by the terrain shader (dug rows, three to a tile, flagged in the tile
    texture), so they follow the ground. The grain is a model per tile, scaled by growth and
    tinted green to gold.
  - **Sites** show scaffolding with logs inside; from half built, the building stands inside it.
  - **Animals** with a skinned model (the deer: a stag and two does) share the villager's
    skinning; others (rabbits) are whole models that hop when they move.
- **Pixel pass** (v1's look, kept):
  - **Pixel size:** 48 art pixels per tile, belonging to the world (24 until M5). An art pixel
    is always a whole number of screen pixels. `COLONY_ART_PER_TILE` overrides it, for
    comparing densities.
  - **Camera snap:** the camera snaps to the art-pixel grid so panning doesn't crawl.
  - **Lines:** outlines where depth jumps, softer lines at creases.
  - **Resolution:** the scene is always drawn at art-pixel resolution, capped at the window's.
    At 48 per tile, mid and far zooms render at full window resolution. Measured on the M3 Pro
    at 3024×1964 with 25 people, the inspector and a lens open: 423 fps at zoom 7, 381 at 16,
    192 at 40, 169 at 120.
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
  - **God toolbar** along the bottom (M10): Speak (to whoever is selected) and Gift. A gift is
    a command: food or materials straight into the stores, or a finished shelter, house or
    storehouse on the next clear spot near the camp (a home goes to someone without one the
    next morning).
  - **Lens buttons** over the world.
  - **Ledgers:** full-screen pages for Society, Economy, Knowledge, History, Map and Arena,
    behind one Ledgers button with a tab for each (M10). Names on them are links to the person.
    - **Society:** households and who lives where, generations (the dead marked), the closest
      bonds and the tensions, and the camp's mood.
    - **Economy (stock only):** each item now, its last 24 mornings as bars (`Stats::days`, kept
      each morning after the night's spoiling), the change over six days and what spoiled; then
      buildings and fields.
    - **Knowledge:** each skill, how many know it, the best at it and everyone else who does;
      taught skills with one knower left are marked at risk.
    - **Map:** the whole region tile by tile, darker where nobody alive has been, with buildings,
      fields, trails, people and the animals where last seen. A click moves the camera there.
- **After the first blind playtest (M10):**
  - **God tools work while paused:** waiting commands are applied at once (at the same tick
    they'd have been applied at anyway, so replays don't change).
  - **Speaking answers back:** the person replies in a bubble ("All right, I'll fetch wood.",
    "Later. My thirst comes first.", "Sorry? I don't follow."), and the story feed says when
    they do what you asked. Phrases are read whole ("nowhere to sleep" means build).
  - **Finding people:** a name clicked anywhere glides the camera to them; Follow keeps them in
    view (any pan, or Esc, stops it); the Mood and Needs lenses list who is low or in need.
  - **The developer view is off by default** (the brain chips, the rules comparison in the Log,
    the Arena button, the frame rate).
  - **The bottom bar** (and the speak box and gift menu) sits in the space left of an open
    inspector; a gift's details show in the menu itself, not a tooltip over the buttons.
- **Menus (M10):** the main menu at launch, over the world from `--seed` going on behind it
  (Continue, New world, Play this one, Load, Settings, Quit); the pause menu on Esc when nothing
  else is open (Resume, Save, New world, Load, Settings, Quit).
  - **New world:** a seed (typed, or rolled), a size (192, 256 or 320 m), whether beauty
    matters, and the packs.
  - **Saves:** five slots and an autosave (each morning, at most every three minutes, and on
    quitting), plus "Your previous world": loading another world or starting a new one keeps
    the one being played there first, so nothing is ever lost to a load. They live in the user's
    Application Support folder (`COLONY_DATA` points elsewhere; tests
    and tools never touch the player's saves). Each slot has a small description beside it.
  - **Settings** (the app's, kept in `settings.json`): power (normal, or lower: no shadows, at
    most 60 frames a second, the rules brain only), the size of text and panels, and full
    screen.
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
  mood and needs put a coloured ring under each person (the selection ring, tinted). The
  traffic lens also shows the trip-time stat. The soil lens (M6) is drawn in the ground shader
  from the tile texture (fertility and moisture).
- **Arena ledger (M9):** the Arena button (developer view) shows the last
  `runs/arena/report.md` with each measure as bars, one colour per brain.
- **World settings (M7):** the World button opens them. "Beauty matters" is the first; changing
  it sends a command, so it's in the command log and replays.
- **Beauty lens (M7):** the renderer puts the beauty field in the tile texture's moisture channel
  while the lens is on.
- **Hobbies (M7)** are on the person's Skills tab: their hobbies, and how close other activities
  are to becoming one.
- **Top bar (M6):** today's sky (and any cold snap) with the temperature; the tooltip gives snow
  cover. The era reads "Era 1, foraging" until the first house or sown field, then "Era 2,
  settling".
- **Story feed (M4):** built from sim events, stamped with the tick they happened on
  (`Sim::take_timed_events`). The same thing happening to several people within three hours
  groups into one line ("3 people are very thirsty").
- **Everything is clickable:** people, resource nodes (trees, bushes, rocks, sticks, flowers),
  graves, the fire and the stores.
  - The cursor picks a person if one is within reach, otherwise the thing whose upright shape on
    screen it's over (`view::pick`).
  - A thing gets the same hover card and inspector panel as a person, with a gold ring.
  - What a thing's inspector shows:
    - **Grave:** who lies there, how they died, who buried them, their family, and who still
      remembers them.
    - **Resource:** what's left, how it grows back, what it's used for, and how many people know
      of it.
    - **Fire:** fuel, who lit it, and how many can relight it.
    - **Stores:** contents and days of food.
    - **House or shelter (M6):** who lives there and who's asleep inside, what it keeps out;
      a site shows how far along it is, what it still needs, and who's working on it.
    - **Field:** the crop's growth, health and weeds, when it'll be ripe, and today's growing
      conditions (moisture, richness, warmth, season), each with what's best.
    - **Storehouse:** how slowly each food spoils with it.
    - **Animal:** what it's doing, its herd, how wary it is, and how its kind is faring in the
      land (count against what the land holds, how many have been hunted).
- **Headless UI shots:** `colony --shot out.png --inspect K --tab log --trace N --lens traffic
  --detail analyst --cursor X,Y` renders any view without a window; `--bench` takes the same
  options to time frames with the UI open.

## 13. Tools and tests

- **`colony-run`:** `run`, `arena`, `map`, `traces`, `export`, `bridge`, `distill` and `bench`
  (sim speed).
- **`colony --drive DIR` (M10):** the real game, played from command files (click, move, key,
  hold, type, scroll, drag, wait, shot, info, quit) through the same input paths as the mouse
  and keyboard. It's how the menus and saves are tested in the window, and how an agent plays
  the game blind. The protocol is in `crates/colony/src/drive.rs`. Its screenshots are at
  window-point size (a pixel is a point you can click), and are taken even when the window is
  covered and gives no image of its own. Playtest reports go in `docs/playtests/`.
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

The lower-power preset (M10, in Settings) drops shadows, caps frames at 60 a second and keeps the
rules brain. A real low-end target comes later.

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
