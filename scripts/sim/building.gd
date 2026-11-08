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


func def() -> Dictionary:
	return Content.get_instance().buildings[type]


func provides(key: String, default: Variant = null) -> Variant:
	return def().get("provides", {}).get(key, default)


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
