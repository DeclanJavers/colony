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

## Building and running

Needs Rust 1.98 or newer.

Run the tests:

```bash
cargo test --release
```

Look at worlds:

```bash
cargo run --release -p colony-run -- maps --seeds 1..10
```

```bash
cargo run --release -p colony-run -- map --seed 2
```

`maps` writes a contact sheet of ten seeds to `runs/maps/`. `map` writes every layer of one seed:
- overview;
- elevation;
- biomes;
- temperature;
- rain;
- moisture;
- fertility;
- water flow.

## Playing

```bash
cargo run --release -p colony
```

| Control | Does |
|---|---|
| Drag, or two-finger scroll | Pan |
| Pinch, or mouse wheel | Zoom |
| WASD / arrows, Z / X | Pan, zoom |
| Space, 1 / 2 / 3 | Pause, speed |
| T / Y | Jump an hour / a day |
| F | Full screen |
| F3 | Hide the FPS readout |

**Starting options:** `--seed N --day D --time H --zoom Z`. For example, `--day 14` starts in
autumn and `--day 20` in winter.

**Checking work without opening a window:**
- `tools/shot.sh out.png [flags]` renders a frame with no window;
- `tools/perf.sh` measures frame times.

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
