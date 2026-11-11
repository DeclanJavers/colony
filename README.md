# Colony

A god-mode sandbox where AI-driven people live, raise families and grow a civilisation. You
watch, help, meddle, and test which minds do best.

**Status: rebuilding from scratch in Rust.** The plan is in `docs/`:

| Doc | What it covers |
|---|---|
| [VISION.md](docs/VISION.md) | What the game is, its pillars, and what the base includes |
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | How it's built: sim, world, people, brains, decision traces, subagent bridge, packs, renderer, UI |
| [ROADMAP.md](docs/ROADMAP.md) | Milestones M0–M10 for the base, each with "done when" checks |
| [LATER.md](docs/LATER.md) | Expansions and improvements after the base |
| [STYLE.md](docs/STYLE.md) | Art direction |

## The first build

The first version (Godot 4.6, GDScript, Laya decisions) is archived in
[archive/godot-v1/](archive/godot-v1/). It still runs on its own. Its README there has the
instructions:

```bash
cd archive/godot-v1 && brain_server/run.sh
```

```bash
Godot --path archive/godot-v1
```
