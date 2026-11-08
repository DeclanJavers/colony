class_name WorldMap
extends RefCounted
## Tile grid, resource nodes, and pathfinding. Generated from a seed.

enum Tile { GRASS, WATER, FORD }

var size: int
var tiles: PackedByteArray
var astar := AStarGrid2D.new()
var node_at: Dictionary = {}        # Vector2i -> ResNode id
var building_at: Dictionary = {}    # Vector2i -> Building id
var shore_cells: Array[Vector2i] = []   # walkable cells next to water (drink/fetch spots)
var center: Vector2i


func setup(map_size: int) -> void:
	size = map_size
	center = Vector2i(size / 2, size / 2)
	tiles = PackedByteArray()
	tiles.resize(size * size)
	astar.region = Rect2i(0, 0, size, size)
	astar.cell_size = Vector2.ONE
	astar.diagonal_mode = AStarGrid2D.DIAGONAL_MODE_ONLY_IF_NO_OBSTACLES
	astar.default_compute_heuristic = AStarGrid2D.HEURISTIC_OCTILE
	astar.default_estimate_heuristic = AStarGrid2D.HEURISTIC_OCTILE
	astar.update()


func in_bounds(c: Vector2i) -> bool:
	return c.x >= 0 and c.y >= 0 and c.x < size and c.y < size


func tile(c: Vector2i) -> int:
	return tiles[c.y * size + c.x]


func set_tile(c: Vector2i, t: int) -> void:
	tiles[c.y * size + c.x] = t
	_refresh_solid(c)


func is_walkable(c: Vector2i) -> bool:
	return in_bounds(c) and not astar.is_point_solid(c)


func _refresh_solid(c: Vector2i) -> void:
	var solid := tile(c) == Tile.WATER or node_at.has(c) or building_at.has(c)
	astar.set_point_solid(c, solid)


func refresh_cell(c: Vector2i) -> void:
	_refresh_solid(c)


## --- Generation -------------------------------------------------------------

func generate(rng: RandomNumberGenerator, sim: Sim) -> void:
	# River: meandering band running top to bottom, with shallow fords to cross.
	var noise := FastNoiseLite.new()
	noise.seed = rng.randi()
	noise.frequency = 0.05
	var river_x := size * 0.22 + rng.randf_range(-2, 2)
	for y in size:
		var cx := int(round(river_x + sin(y * 0.13) * 3.0 + noise.get_noise_1d(y) * 4.0))
		for dx in range(-1, 2):
			var c := Vector2i(cx + dx, y)
			if in_bounds(c):
				set_tile(c, Tile.FORD if y % 14 == 7 else Tile.WATER)

	# Forests and rock fields from noise, keeping the colony centre clear.
	var forest := FastNoiseLite.new()
	forest.seed = rng.randi()
	forest.frequency = 0.07
	var rocks := FastNoiseLite.new()
	rocks.seed = rng.randi()
	rocks.frequency = 0.09
	for y in size:
		for x in size:
			var c := Vector2i(x, y)
			if tile(c) != Tile.GRASS:
				continue
			var d := Vector2(c - center).length()
			if d < 7.0:
				continue
			if forest.get_noise_2d(x, y) > 0.22 and rng.randf() < 0.55:
				sim.spawn_node("tree", c)
			elif rocks.get_noise_2d(x, y) > 0.42 and rng.randf() < 0.5:
				sim.spawn_node("rock", c)
			elif rng.randf() < 0.012:
				sim.spawn_node("berry_bush", c)

	# Guarantee a few of each within reach of the start, so a seed can't be unwinnable.
	_scatter_ring(rng, sim, "tree", 12, 8.0, 16.0)
	_scatter_ring(rng, sim, "rock", 6, 9.0, 17.0)
	_scatter_ring(rng, sim, "berry_bush", 8, 6.0, 13.0)
	rebuild_shore()


func _scatter_ring(rng: RandomNumberGenerator, sim: Sim, type: String, count: int, r0: float, r1: float) -> void:
	var placed := 0
	var tries := 0
	while placed < count and tries < count * 40:
		tries += 1
		var a := rng.randf() * TAU
		var r := rng.randf_range(r0, r1)
		var c := center + Vector2i(int(cos(a) * r), int(sin(a) * r))
		if in_bounds(c) and tile(c) == Tile.GRASS and is_walkable(c):
			sim.spawn_node(type, c)
			placed += 1


func rebuild_shore() -> void:
	shore_cells.clear()
	for y in size:
		for x in size:
			var c := Vector2i(x, y)
			if not is_walkable(c):
				continue
			for n in neighbors4(c):
				if in_bounds(n) and tile(n) == Tile.WATER:
					shore_cells.append(c)
					break


## --- Queries ----------------------------------------------------------------

func neighbors4(c: Vector2i) -> Array[Vector2i]:
	return [c + Vector2i.LEFT, c + Vector2i.RIGHT, c + Vector2i.UP, c + Vector2i.DOWN]


func neighbors8(c: Vector2i) -> Array[Vector2i]:
	var out: Array[Vector2i] = []
	for dy in range(-1, 2):
		for dx in range(-1, 2):
			if dx != 0 or dy != 0:
				out.append(c + Vector2i(dx, dy))
	return out


func access_cell(targets: Array[Vector2i], from: Vector2i) -> Vector2i:
	## Nearest walkable cell to stand on to use something occupying `targets`.
	## If a target cell itself is walkable (farm plot), standing on it is fine.
	var best := Vector2i(-1, -1)
	var best_d := INF
	for t in targets:
		var cands: Array[Vector2i] = []
		if is_walkable(t):
			cands.append(t)
		else:
			cands = neighbors8(t)
		for n in cands:
			if not is_walkable(n):
				continue
			var d := Vector2(n - from).length_squared()
			if d < best_d:
				best_d = d
				best = n
	return best


func find_path(from: Vector2i, to: Vector2i) -> Array[Vector2i]:
	## Path excluding the start cell. Empty if already there or unreachable.
	if from == to or not in_bounds(to):
		return []
	var p: Array[Vector2i] = astar.get_id_path(from, to)
	if p.size() > 0:
		p.remove_at(0)
	return p


func nearest_walkable(from: Vector2i) -> Vector2i:
	if is_walkable(from):
		return from
	for r in range(1, 6):
		for dy in range(-r, r + 1):
			for dx in range(-r, r + 1):
				var c := from + Vector2i(dx, dy)
				if is_walkable(c):
					return c
	return from
