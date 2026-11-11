# Colony: after the base

Everything here waits until the base ([ROADMAP.md](ROADMAP.md)) is signed off. Each item is an
expansion pack or an improvement. The base already keeps the quantities and hooks each one needs,
noted in brackets. Order isn't fixed: we pick the next one each time a stage is done.

## Expansions, roughly by era

**Era 2–3: settling, tools, organisation**
- **Farming:** [soil moisture, fertility, temperature]
  - crop varieties with different water, soil and season needs;
  - irrigation ditches and wells;
  - crop rotation and manure;
  - plows and draft animals;
  - storage and spoilage;
  - orchards.
- **Infrastructure:** [footfall, path costs, water tiles]
  - planned roads that pave the worn trails, decided by the settlement;
  - fords (slow, seasonal), log bridges, then stone bridges, ferries and docks;
  - wells and aqueducts;
  - signposts and milestones;
  - walls;
  - a trip-time stat per job to show the gain.
- **Tools and crafting:** material tiers (stone, copper, bronze, iron), workshops, tool wear and
  quality, specialists.
- **Domestic animals:** dogs (hunting, guarding), sheep, goats, cows, chickens, horses. Taming
  from wild populations.
- **Better homes:** house tiers, furniture, and ownership of homes and goods. [owner field]

**Era 4: government and power**
- **Government:** [groups, beliefs]
  - forms: chiefdom, council, monarchy, democracy;
  - elections, laws, taxes, crime and justice;
  - factions and power struggles;
  - economic systems (communal, market, planned) that you can install or let them choose.
- **Religion and myth:** [memory, hearsay, beliefs, events]
  - god-power events become stories that drift as they're retold;
  - faiths form around them, with shrines, rituals and priests;
  - your interventions shape what they believe about you.
- **Writing and schools:** knowledge that survives its holders, books, teachers.

**Era 5: complex society**
- **Economy and trade:** money emerging from barter, prices, markets, merchants, trade routes.
- **Mines and quarries,** deeper resources.
- **Dedicated roles:** farmer, teacher, cook, smith, governor; roles chosen and changed by the
  society.
- **Multiple settlements:**
  - groups break away with their own systems;
  - diplomacy, trade, war;
  - culture and dialect drift;
  - far settlements simulated in less detail. [chunks]
- **Seen-world maps:** each culture knows only what it has explored or heard; maps are traded.
  [per-person knowledge]
- **Boats and carriages:** sea and river travel, roads for carts. [water tiles, path costs]

**Across eras**
- **Disease, medicine and sanitation:** [health fields, events]
  - illness spreading through contact and water;
  - latrines and clean wells;
  - healers, with medicine as knowledge.
- **God powers:** meteors, earthquakes, floods, drought, fire, plagues; zombies and other enemies;
  blessings; inspiring or tricking people; pitting people against each other.
- **Ecosystems, deeper:** predators (wolves, bears), fish populations, forest growth and clearing,
  climate change across years. [animal populations, biomes]

## Improvements to the base

- **Brains:**
  - periodic Laya fine-tunes;
  - a fine-tuned small chat model trained on subagent dialogue;
  - outcome-based training (learning from what worked, not just copying the teacher);
  - DAgger rounds on the distilled brains' own mistakes;
  - brains for whole settlements (councils, leaders).
- **Lies and exaggeration** in what people tell each other, driven by motives (rivalry,
  self-interest, blame).
- **Observer views:** the relationship web, more lenses (wealth, belief, disease, ownership),
  replays and time-lapse.
- **Hardware:** a real low-end target, below the M3 Pro preset.
- **Modding:** third-party packs through WASM, and a pack browser.
- **Audio:** music that follows the mood of the settlement, and ambient sound by biome and
  season.
- **Bigger worlds:** several regions and streaming chunks.

## Not doing

- Caves.
- Paid model APIs (subagents through Claude Code instead).
