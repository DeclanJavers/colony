class_name Building
extends RefCounted
## A placed building or construction site. Type data comes from data/buildings.json.

var id: int
var type: String
var cell: Vector2i          # top-left of footprint
var size: Vector2i = Vector2i.ONE
var complete: bool = false
var work_done: float = 0.0
var work_needed: float = 1.0
var beds: Array = []        # colonist ids (or -1) per bed
var farm_stage: String = "empty"   # empty | growing | ripe
var farm_growth: float = 0.0       # 0..1
var reserved_by: int = -1          # colonist currently working it (farms)
var placed_at: float = -1.0        # sim clock (minutes)
var completed_at: float = -1.0
var placed_by: String = ""         # colonist name ("" = there from the start)
var work_by: Dictionary = {}       # colonist name -> minutes of building work put in
var harvests: int = 0              # farms: times harvested
var heated: bool = false           # houses: burning firewood right now
var level: int = 1                 # buildings with "levels" (meeting hall): how far it's been enlarged
var expanding: bool = false        # being enlarged to the next level: still usable, work_done counts up
var prev_cell: Vector2i = Vector2i(-1, -1)   # footprint before the enlargement started (for drawing)
var prev_size: Vector2i = Vector2i.ZERO
var grow_rect := Rect2i()          # land kept free for the building's biggest level (empty = none)


func def() -> Dictionary:
	return Content.get_instance().buildings[type]


func provides(key: String, default: Variant = null) -> Variant:
	return def().get("provides", {}).get(key, default)


## Level data ({seats, footprint, cost, work}) for `lv` (default: the current level), or {}.
func level_def(lv: int = -1) -> Dictionary:
	var levels: Array = def().get("levels", [])
	var i := (level if lv < 0 else lv) - 1
	return levels[i] if i >= 0 and i < levels.size() else {}


func seats() -> int:
	return int(level_def().get("seats", 0))


## The land this building occupies or keeps free to grow into.
func land() -> Rect2i:
	var r := Rect2i(cell, size)
	return r.merge(grow_rect) if grow_rect.size != Vector2i.ZERO else r


## Still being built or enlarged (builders can work on it).
func is_site() -> bool:
	return not complete or expanding


func cells() -> Array[Vector2i]:
	var out: Array[Vector2i] = []
	for y in size.y:
		for x in size.x:
			out.append(cell + Vector2i(x, y))
	return out


func center() -> Vector2:
	return Vector2(cell) + Vector2(size - Vector2i.ONE) * 0.5


func progress() -> float:
	return clampf(work_done / maxf(work_needed, 0.001), 0.0, 1.0)


func to_dict() -> Dictionary:
	return {"id": id, "type": type, "cell": [cell.x, cell.y], "complete": complete,
		"progress": progress(), "farm_stage": farm_stage, "farm_growth": farm_growth, "beds": beds}
