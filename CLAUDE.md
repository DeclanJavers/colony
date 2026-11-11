# Colony

- **Rebuild in progress (Rust).** Read `docs/VISION.md`, `docs/ARCHITECTURE.md` and `docs/ROADMAP.md`
  before working. Work follows the roadmap's milestones. A milestone's "done when" checks must pass
  and the user signs off before the next starts. When the plan changes, update the docs first.
- **The old build** lives in `archive/godot-v1/` (Godot 4.6), kept for reference and comparison.
  Don't develop it further. Its lessons are in ARCHITECTURE.md section 16.
- **Decision traces are required.** Every brain decision records what the brain saw, every
  option's score, the choice and the reason (ARCHITECTURE.md section 7).
- **Strong models run as Claude Code subagents (Haiku), never paid APIs.** Heavy training goes to
  Colab on the cheapest GPU (T4). Only tiny models (the scorer) train on the Mac.
- **Art style:** every new or changed asset follows [docs/STYLE.md](docs/STYLE.md). Check new art
  in game with `tools/shot.sh` (small, background window on screen 0) before calling it done.
