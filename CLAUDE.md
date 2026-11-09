# Colony

- **Art style:** every new or changed asset (models, textures, shaders, effects, UI art) follows
  [docs/STYLE.md](docs/STYLE.md). Models come from `tools/blender/make_models.py`; check new art in
  game with `tools/shot.sh` (it opens on screen 0) before calling it done. After rebuilding models,
  run `Godot --headless --path . --import` so the game picks them up.
