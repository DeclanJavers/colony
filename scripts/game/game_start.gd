class_name GameStart
extends RefCounted
## What the menu hands to the game scene: a new game's settings, and which save slot this game
## saves to.

## New game: {seed: int, map_options: Dictionary (WorldMap.DEFAULT_OPTIONS + size, settlers),
## brain: String}. Empty = start the way command-line runs do.
static var new_game: Dictionary = {}
## Save slot this game saves to ("" = a new slot on first save).
static var slot_id: String = ""
