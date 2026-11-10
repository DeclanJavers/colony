class_name FitGrid
extends RefCounted
## Fast "does a building fit here?" for the building spot search (Sim._fits). Built once per search
## from the map: summed-area tables of blocked cells (buildings, trees, rocks), non-grass cells and
## each terrain level, so checking a footprint is a few lookups instead of a loop over its cells.
## Same answer as the cell-by-cell check.

var n := 0
var _blocked := PackedInt32Array()
var _nongrass := PackedInt32Array()
var _level: Array[PackedInt32Array] = []
var _map: WorldMap


func _init(map: WorldMap) -> void:
	_map = map
	n = map.size
	var top := 0
	for v in map.height:
		top = maxi(top, v)
	var blocked := PackedByteArray()
	blocked.resize(n * n)
	for c: Vector2i in map.building_at:
		blocked[c.y * n + c.x] = 1
	for c: Vector2i in map.node_at:
		blocked[c.y * n + c.x] = 1
	var nongrass := PackedByteArray()
	nongrass.resize(n * n)
	for i in n * n:
		nongrass[i] = 1 if map.tiles[i] != WorldMap.Tile.GRASS else 0
	_blocked = _sat(blocked, -1)
	_nongrass = _sat(nongrass, -1)
	for h in top + 1:
		_level.append(_sat(map.height, h))


## Summed-area table of `cells` (or, with `eq` >= 0, of cells equal to `eq`).
func _sat(cells: PackedByteArray, eq: int) -> PackedInt32Array:
	var w := n + 1
	var s := PackedInt32Array()
	s.resize(w * w)
	for y in n:
		var row := 0
		var base := y * n
		for x in n:
			var v := cells[base + x]
			row += (1 if v == eq else 0) if eq >= 0 else v
			s[(y + 1) * w + x + 1] = s[y * w + x + 1] + row
	return s


## Sum over cells x0..x1-1, y0..y1-1.
func _sum(s: PackedInt32Array, x0: int, y0: int, x1: int, y1: int) -> int:
	var w := n + 1
	return s[y1 * w + x1] - s[y0 * w + x1] - s[y1 * w + x0] + s[y0 * w + x0]


## The footprint at `origin` plus its 1-tile walkway ring is on the map and free of buildings and
## nodes; the footprint itself is all grass and all one level (the origin's).
func fits(origin: Vector2i, size: Vector2i) -> bool:
	var x0 := origin.x - 1
	var y0 := origin.y - 1
	var x1 := origin.x + size.x + 1
	var y1 := origin.y + size.y + 1
	if x0 < 0 or y0 < 0 or x1 > n or y1 > n:
		return false
	if _sum(_blocked, x0, y0, x1, y1) > 0:
		return false
	var ix1 := origin.x + size.x
	var iy1 := origin.y + size.y
	if _sum(_nongrass, origin.x, origin.y, ix1, iy1) > 0:
		return false
	var h := _map.height_at(origin)
	return h < _level.size() and _sum(_level[h], origin.x, origin.y, ix1, iy1) == size.x * size.y
