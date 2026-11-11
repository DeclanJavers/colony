# Colony: vision

**A god-mode sandbox where AI-driven people live, raise families and grow a civilisation, from a
handful of foragers to nations. You watch, help, meddle, and test which minds do best.**

This file says what the game is and the rules every decision follows. [ARCHITECTURE.md](ARCHITECTURE.md)
says how it's built, [ROADMAP.md](ROADMAP.md) says in what order, and [LATER.md](LATER.md) holds
everything that comes after the base.

## What you do

- **Watch.** A living world with people who need, plan, love, argue, learn, age and die.
- **Inspect.** Click anyone and see their thoughts, needs, family, memories, DNA, skills and
  hobbies, plus every decision they made and exactly why.
- **Help or meddle.** Talk to people (good advice or bad), give gifts. Later: disasters,
  enemies, installing governments, pitting people against each other.
- **Experiment.** Put different minds (brains) in charge of different people or settlements, run
  the same world twice, and compare what they build. Do they plant flowers when it does nothing
  for them?

## Pillars

1. **Organic, not scripted.** Behaviour comes from needs, knowledge, personality, relationships and
   the brain. Nobody hand-places stories, paths or roles. When behaviour is wrong, fix the teacher,
   the data or the evaluation: never a hard cap.
2. **Clear.** Dense information shown calmly, in four levels: the world itself, hover cards,
   the inspector, and full-screen ledgers. Every number explains itself on hover. Every decision
   keeps a trace of what the brain saw and what it picked (see ARCHITECTURE: Decision traces).
3. **Real.** People inherit DNA from their parents. The world is real geography from a seed:
   biomes, hills, mountains, rivers, lakes, seas. Knowledge lives in people and can be lost.
   Animals have populations that can collapse.
4. **A lab for minds.** One brain plug, many brains: hand-written rules, Claude Haiku
   subagents (through Claude Code, no API), and our own small models distilled from them. Same
   seed plus a different brain gives a fair comparison.
5. **A simple base, rich expansions.** The base models the underlying quantities simply (soil
   moisture, fertility, health, ownership, groups). Each expansion adds new ways to change those
   quantities, so it deepens the world instead of bolting on a menu.
6. **Runs well.** At least 120 fps on a MacBook Pro M3 Pro at normal power, full screen. Headless
   runs are fast enough to generate training data and run comparisons. An AI budget setting runs
   everything from pure code (any laptop) up to strong models.

## The base

The base is the first complete, satisfying game. It covers era 1 (foraging) and the start of era 2
(settling), in one generated region with one starting group of about 10 people. Everything needed
to watch, understand and experiment is in it:

- a generated region: terrain, water, biomes, climate, seasons, weather, day and night;
- people with DNA, bodies, needs, mood, personality, skills, knowledge, memory, relationships and
  families across generations;
- foraging, water, wood and stone, fire, shelter, houses and storage, and simple hunting;
- basic farming: one crop, driven by soil moisture, fertility and season;
- desire paths: busy routes wear into trails on their own;
- leisure, hobbies and beauty: flowers, carvings, decoration, with a switch to make beauty matter
  or not;
- talking, sharing news, asking for help, teaching;
- the brain plug with rules, subagent and distilled brains; chat; the AI budget setting;
- the full information UI, lenses, the chronicle, decision traces, world branching, and the
  brain arena;
- god tools: speak and gift.

Everything else is an expansion, listed in [LATER.md](LATER.md).

## What we won't do

- **Caves.**
- **Paid model APIs.** Strong models run as Claude Code subagents on the user's own plan, with
  the game paused while they think.
- **Hard caps or scripted fixes** in place of better behaviour.
- **Clutter.** Detail is always one hover or click away, never piled on screen.

## How we work

Each milestone in the roadmap ends with "done when" checks: some measured, some played. The user
plays it and signs off before the next milestone starts. When the plan changes, these docs change
first.

A full rebuild. The first build (Godot, 2026-11) is archived in `archive/godot-v1/` with its
lessons written into ARCHITECTURE.md.
