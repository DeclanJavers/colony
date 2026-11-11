# Colony art style

Every new or changed asset (models, textures, shaders, effects, UI art) follows this guide. When an
asset and this guide disagree, change the asset, or change the guide on purpose.

## The look in one line

**A cozy storybook village, seen as a little isometric diorama, in chunky pixel art.**
Half-timbered houses with steep tiled roofs, cobbles worn in by footsteps, round leafy trees,
crates and barrels, all sitting on a block of earth floating on a dark background.

Reference screenshots are in `docs/style/` (`day`, `night`, `autumn`, `winter`, `map`). They come
from the first build (`archive/godot-v1`), and the rebuild keeps this look. New work should sit
comfortably next to them.

## How we get it in 3D

We build in 3D (our Rust renderer, see ARCHITECTURE.md section 11) and make it read as pixel art:

1. **Simple low-poly shapes.** Blocky forms. Nothing thinner than about 0.05 m: it vanishes once
   pixelated. Put detail where the game camera sees it: beams, shutters, tile rows, stones.
2. **Flat colours from the palette below.** No photo textures, no gradients baked into models, at
   most a base and a darker accent per material. Light and shadow come from the renderer.
3. **The pixel pass** (`crates/render`: the scene is drawn into a low-resolution target, then
   `shaders/post.wgsl` scales it up) draws the world in big pixels
   that belong to the world, like classic pixel art: about 24 art pixels per tile, so they grow when
   you zoom in and shrink when you zoom out.
   An art pixel is always a whole number of rendered pixels, and the camera snaps to the art-pixel
   grid, so panning moves the picture a whole art pixel at a time (no crawling or shimmer). It puts dark outlines where
   a shape ends against something further away, adds a softer line where surfaces meet at an angle,
   and applies the night grade. Colour banding is available (`bands`) but off: flat palette colours
   already band. Judge assets *through* the pixel pass in game, never in Blender.
4. **Text, icons, speech bubbles, smoke and snow stay sharp.** They draw after the pixel pass.
   (Rain isn't drawn: it made the view busy. Rainy weather shows in the top bar.)

## Palette

Warm and saturated, never neon. Shadows lean cool and blue rather than black. sRGB hex.

| Use | Base | Others |
|---|---|---|
| Roof, red tile | `#B8432F` | row `#A33A29`, ridge `#8A2E22` |
| Roof, blue slate | `#3F67A8` | row `#375B96`, ridge `#2D4C80` |
| Plaster wall | `#EAD9B0` | |
| Timber (beams, frames) | `#5A3620` | |
| Planks (doors, decks, crates) | `#9C6B3F` | dark `#7A4F2C` |
| Stone (footings, wells, cobbles) | `#9A9590` | dark `#6E6A68`, light `#B8B3AA` |
| Shutters | `#4E7A6A` (green) | `#4B6A9A` (blue) |
| Window glass (unlit) | `#2B3550` | lit: `#FFB040` |
| Iron (hoops, anvils, lamps) | `#3A3A40` | |
| Grass (spring) | `#5C9439` | dark `#4E8233`, light `#6CA443` |
| Grass (summer / autumn / winter) | `#578E35` / `#86913D` / `#6F8A5C` under snow | |
| Snow on the ground | `#BDC9DB` to `#D6E0ED` | on roofs `#F7FAFF` (brighter, so roofs stand out) |
| Leaves (summer) | `#3F8A35` | light top `#63AD45` |
| Leaves (autumn, broadleaf only) | `#D9822B`, `#B9472A`, `#E3B23C` | pines stay green |
| Pine | `#2F6E3A` | top `#24573A` |
| Soil, dirt path | `#8A5A35` | dark `#5C3A22` |
| Crops | `#7DB040` | ripe: gold `#F2C640` |
| Water | `#3C7FC0` | ripple `#7FB8E6`, shore sand `#CCB880` |
| Fire, lamps, lit windows | `#FFB040` | flame core `#FFE08A` |
| Berries | `#C8283C` | |
| Wildflowers | `#F2C640` (yellow) | white `#F3EADB`, violet `#9A7FD1`, red `#C8283C`; leaves `#63AD45` |
| Reeds | `#7FA347` | heads `#7A4F2C` |
| Sacking (food sacks) | `#D2B48C` | water barrels `#557FAE` |
| Villager skin | `#E8BC98` | |
| Outline | `#2A1E1A` | never pure black |
| Selection ring | `#E8C468` | |
| Background (void around the diorama) | `#1E1A1F` | faintly tinted by the sky |

Colonists' clothes are the only free colours: one hue per person, kept in the same saturation band.
A new colour goes into this table before it goes into a model.

## Shapes, proportions and variety

- **1 tile = 1 m.** Each model's origin is the centre of its footprint at ground level; its front
  (door) faces -Y in Blender (+Z in the game, Y up).
- **Buildings:** stone footing (~0.25 m), plaster walls with timber framing (corner posts, top and
  bottom beams, a mid rail, braces on the long walls), shuttered windows, a framed plank door with a
  stone step. Roofs are steep (about 45°) with a 0.15 m overhang, visible tile rows, barge boards, a
  ridge beam, and usually a chimney. Walls about 1 m tall; ridge about 2.1 m.
- **Variety:** every building type that repeats has variants (`house`, `house_2`...): at least one
  in four houses has a blue roof, and shutters and props differ. The game also turns square
  buildings to face different ways. A street of identical clones is a bug.
- **Trees:** broadleaf trees are lumpy canopies of 6–8 overlapping leaf clumps on a short trunk,
  with lighter clumps on top towards the light (back-left). Pines are stacked cones. Mix both.
  Trees are a little taller than a house's walls.
- **Props:** crates, barrels, sacks, lamp posts, fences. Small and chunky, used to make places look
  lived in. Keep them inside the building's own footprint so they never poke into a neighbour.
- **Characters:** low-poly villagers with readable silhouettes and limbs; they keep their own colour.
  Anything they carry is shown big enough to read at normal zoom.

## Ground

- Grass is a shader at 16 texels per tile: soft clumps of darker and lighter green, a few specks,
  and small "v" tufts. Low contrast, no visible grid.
- **Paths emerge from footsteps** (the footfall field): grass wears to dirt where people walk often,
  and the busiest ground near buildings becomes cobblestone. Nobody paints roads.
- The map is a **diorama block**: the sides show a ragged turf lip over layered brown soil with a
  few stones; rivers that reach the edge show as water there. It floats on the dark background.

## Water

- **Waterline:** wherever the smooth ground meets the water, never along tile edges.
  - **Shallows:** fade from sand to blue over the last 20 cm.
  - **Depth:** deeper water darkens.
  - **Foam:** a thin line sits right at the edge.
- **Current:** the water is one continuous ripple pattern carried along the flow (a flow map).
  - **Rivers:** run downhill, faster where narrow, steep or full, slower at the banks.
  - **At a mouth:** the current carries on as a fading plume into the sea or lake.
  - **Calm water:** drifts slowly with a breeze, with soft crests and the odd glint.
  - **Fast water:** brighter crests and white flecks.
- **Pixel art:** all of it uses stepped shades, so it stays crisp under the pixel pass.
- **Checking water work:**
  - `colony-run water --seed N` lists river mouths, lake inlets and confluences;
  - `colony --qa-sheet out.png --spots "N:x,z ..."` renders them all on one sheet;
  - `colony-run tiles --seed N --x X --y Y` prints the tile data when something looks wrong.

## Seasons, weather and night

- Seasons recolour the grass and leaves (spring fresh, summer deep, autumn ochre ground with orange,
  red and gold broadleaves). Winter covers the ground in snow (pale blue-grey, never pure white)
  and puts brighter snow on roofs, but ridges stay dark so roof shapes still read.
- Rain isn't drawn as particles (it made v1 busy): it shows in the light, wet ground and the top
  bar. Snow is soft flakes that follow the camera.
- Night is a cool blue grade from the pixel pass. Warm lights keep their colour: windows glow in
  the evening and before dawn wherever someone lives (and in the kitchen and town hall), then go
  dark while everyone sleeps; lamps burn all night; fires light the ground around them.
- Weather and night may dim the colony but must never make it hard to read.

## Labels and icons (readability first)

- Names, icons and bubbles stay a fixed size on screen at every zoom and never overlap each other:
  spacing between them is in screen pixels, not world units.
- White text always has a thick outline in the outline colour, so it reads on snow as well as grass.
- Zoomed far out, only danger icons show; activity icons appear as you zoom in.
- On-screen UI panels are dark, rounded and translucent, with light text.

## Making new assets

- Models come from the Blender pipeline in `tools/blender/make_models.py`. Use its helpers (`box`, `beam`, `blob`, `tiled_roof`, `timber_face`,
  `window`, `door`, `crate`, `barrel`...) and its palette `P`, which mirrors the table above.
  Build one model only: `Blender -b -P tools/blender/make_models.py -- assets/models house_2`
- Name the parts the game changes: `window` (lit at night), `leaves1`/`leaves2` (seasonal colour),
  `flame`, `lamp`; roof parts keep their `roof_*` material names (winter snow). Everything else is
  merged per material to keep draw calls low.
- Variants are `<name>_2.glb`, `<name>_3.glb`..., picked per object automatically.
- Check every new asset in game with `tools/shot.sh` at
  normal zoom (`--zoom 16`), close (`--zoom 7`), whole map (`--zoom 120`), at night (`--time 21`) and
  in winter, before calling it done. `tools/shot.sh` renders with no window at all:
  `tools/shot.sh runs/shots/x.png --zoom 9 --day 8 --time 21` (flags: `--seed --day --time --zoom --x --z`;
  `colony-run find --seed N --kind <id>` finds a spot to look at).
