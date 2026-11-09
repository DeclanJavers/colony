class_name ColonyWildlife
extends RefCounted
## Game animals (data/animals.json): rabbits in warrens and deer in herds, living in the sim so
## people can see, remember, talk about and hunt them. (Birds, fish, butterflies stay scenery in
## WildlifeView.)
##
## Each group has a home spot it grazes around; the home drifts now and then. Animals bolt from
## anyone who comes too close (a hunter stalking them gets much nearer). In spring and summer groups
## have young, up to what the land can carry; when numbers are low a few more wander in from the
## edge of the map, so hunted-out land slowly recovers. Every kill makes that kind warier for a
## while and moves the group away, so over-hunting gets harder as well as thinning the herds.
##
## Uses its own random numbers so animals don't change the rest of the colony's dice.

var sim: Sim
var rng := RandomNumberGenerator.new()
## id -> {id, kind, group, pos: Vector2, target: Vector2, state: "idle"|"move"|"flee", timer}
var animals: Dictionary = {}
## id -> {id, kind, home: Vector2i, drift_at: float}
var groups: Dictionary = {}
var capacity: Dictionary = {}     # kind -> how many the land carries
var fear: Dictionary = {}         # kind -> extra wariness from recent kills (0 = calm)
var stats := {"kills": {}, "born": {}, "arrived": {}, "hunts": 0, "misses": 0}
var _next_id := 1


func cfg() -> Dictionary:
	return sim.content.animals


func kind_def(kind: String) -> Dictionary:
	return cfg().get("kinds", {}).get(kind, {})


func kinds() -> Array:
	return cfg().get("kinds", {}).keys()


func setup(p_sim: Sim) -> void:
	sim = p_sim
	rng.seed = sim.seed_value * 31 + 7
	var amount := float(sim.map_options.get("game", 1.0))
	for kind in kinds():
		var d := kind_def(kind)
		fear[kind] = 0.0
		var n := roundi(float(d.get("groups_per_100_tiles", 0.1)) * sim.map.size * sim.map.size / 100.0 * amount)
		capacity[kind] = maxi(n, 1 if amount > 0.0 else 0) * int(d.get("capacity_per_group", 5))
		for i in n:
			var home := _home_cell(kind, Vector2i(-1, -1))
			if home.x < 0:
				continue
			var g := _new_group(kind, home)
			var size: Array = d.get("group_size", [2, 3])
			for k in rng.randi_range(int(size[0]), int(size[1])):
				_spawn(kind, g, _near_walkable(home, 2))


## ---- Each sim minute -------------------------------------------------------------

func update(dt: float) -> void:
	if animals.is_empty():
		return
	var people: Array = []
	for c: Colonist in sim.colonists.values():
		people.append([c.pos, bool(c.get_meta("stalking", false)), c.skill_mult("hunting")])
	var stalk := float(cfg().get("stalk_factor", 0.45))
	for a: Dictionary in animals.values():
		var d := kind_def(a["kind"])
		var wary := float(d.get("wary", 3.0)) * (1.0 + float(fear.get(a["kind"], 0.0)))
		var here := Vector2i(a["pos"].round())
		if not sim.map.is_walkable(here):
			# Something was built where it stood (or an old save put it there): step off.
			var w := sim.map.nearest_walkable(here)
			if w.x >= 0:
				a["pos"] = Vector2(w)
				a["target"] = a["pos"]
		var pos: Vector2 = a["pos"]
		if a["state"] != "flee":
			var threat := Vector2.INF
			var best := INF
			for p in people:
				var dist: float = (p[0] as Vector2).distance_to(pos)
				var reach: float = wary * (stalk / float(p[2]) if p[1] else 1.0)
				if dist < reach and dist < best:
					best = dist
					threat = p[0]
			if threat != Vector2.INF:
				scare(a, threat)
		_move(a, d, dt)


func _move(a: Dictionary, d: Dictionary, dt: float) -> void:
	match a["state"]:
		"idle":
			a["timer"] = float(a["timer"]) - dt
			if float(a["timer"]) <= 0.0:
				var g: Dictionary = groups.get(a["group"], {})
				var home: Vector2i = g.get("home", Vector2i(a["pos"].round()))
				var roam := int(d.get("roam", 3))
				var t := _near_walkable(home, roam)
				if t.x >= 0:
					a["target"] = Vector2(t)
					a["state"] = "move"
				else:
					a["timer"] = rng.randf_range(5.0, 20.0)
		"move", "flee":
			var speed := float(d.get("flee_speed" if a["state"] == "flee" else "wander_speed", 0.1))
			var to: Vector2 = a["target"] - a["pos"]
			var step := speed * dt
			if to.length() <= step:
				a["pos"] = a["target"]
				_rest(a)
				return
			var nxt: Vector2 = a["pos"] + to.normalized() * step
			if sim.map.is_walkable(Vector2i(nxt.round())):
				a["pos"] = nxt
			else:
				# Something's in the way (trees, a house, water): try going round it.
				for turn in [0.6, -0.6, 1.2, -1.2]:
					var alt: Vector2 = a["pos"] + to.normalized().rotated(turn) * step
					if sim.map.is_walkable(Vector2i(alt.round())):
						a["pos"] = alt
						return
				_rest(a)


func _rest(a: Dictionary) -> void:
	a["state"] = "idle"
	# Grazing: rabbits nibble a few minutes at a time, deer stand longer.
	a["timer"] = rng.randf_range(4.0, 25.0) if a["kind"] == "rabbit" else rng.randf_range(10.0, 45.0)


## The animal bolts away from `from`.
func scare(a: Dictionary, from: Vector2) -> void:
	var d := kind_def(a["kind"])
	var away: Vector2 = (a["pos"] - from).normalized()
	if away == Vector2.ZERO:
		away = Vector2.from_angle(rng.randf() * TAU)
	var dist := float(d.get("flee_tiles", 5))
	var best := Vector2i(-1, -1)
	for turn in [0.0, 0.5, -0.5, 1.0, -1.0, 1.6, -1.6]:
		var c := Vector2i((a["pos"] + away.rotated(turn) * dist).round())
		if sim.map.in_bounds(c) and sim.map.is_walkable(c):
			best = c
			break
	if best.x < 0:
		return
	a["target"] = Vector2(best)
	a["state"] = "flee"


## ---- Each day: young, newcomers, calming down, moving on ----------------------------------

func on_day() -> void:
	var half := float(cfg().get("fear_half_life_days", 3.0))
	for kind in kinds():
		fear[kind] = float(fear.get(kind, 0.0)) * pow(0.5, 1.0 / maxf(half, 0.1))
	var by_group := {}
	for a: Dictionary in animals.values():
		by_group[a["group"]] = int(by_group.get(a["group"], 0)) + 1
	for gid in groups.keys():
		if int(by_group.get(gid, 0)) == 0:
			groups.erase(gid)      # all gone: the warren or herd is no more
	var breed := float(sim.season_def().get("breed", 0.0))
	for g: Dictionary in groups.values():
		var d := kind_def(g["kind"])
		if sim.clock >= float(g["drift_at"]):
			var h := _home_cell(g["kind"], g["home"], 8)
			if h.x >= 0:
				g["home"] = h
			g["drift_at"] = sim.clock + float(d.get("home_drift_days", 2.0)) * 1440.0 * rng.randf_range(0.6, 1.4)
		var room := 1.0 - float(count(g["kind"])) / maxf(1.0, float(capacity.get(g["kind"], 1)))
		if int(by_group.get(g["id"], 0)) >= 2 and room > 0.0 and rng.randf() < float(d.get("breed_per_group_day", 0.3)) * breed * room:
			_spawn(g["kind"], g["id"], _near_walkable(g["home"], 1))
			stats["born"][g["kind"]] = int(stats["born"].get(g["kind"], 0)) + 1
	# Hunted-out land: a few more wander in from the edge of the map.
	for kind in kinds():
		if float(count(kind)) < float(cfg().get("arrive_below_share", 0.35)) * float(capacity.get(kind, 0)) \
				and rng.randf() < float(cfg().get("arrive_chance_per_day", 0.25)):
			var edge := _edge_cell()
			if edge.x < 0:
				continue
			# They settle not far in from where they came (somewhere they can walk to).
			var home := _home_cell(kind, edge, 10)
			if home.x < 0 or sim.map.find_path(edge, home).is_empty():
				continue
			var g := _new_group(kind, home)
			var came := 0
			for k in 2:
				var a := _spawn(kind, g, _near_walkable(edge, 1))
				if not a.is_empty():
					a["target"] = Vector2(_near_walkable(home, 2))
					a["state"] = "move"
					came += 1
			stats["arrived"][kind] = int(stats["arrived"].get(kind, 0)) + came
			sim.events.animals_arrived.emit(kind, home)


## ---- Hunting -----------------------------------------------------------------------

## The nearest animal (of `which` kinds, all if empty) within `radius` of `pos`, or {}.
func nearest(pos: Vector2, radius: float, which: Array = [], skip: Dictionary = {}) -> Dictionary:
	var best := {}
	var best_d := radius
	for a: Dictionary in animals.values():
		if (not which.is_empty() and not which.has(a["kind"])) or skip.has(a["id"]):
			continue
		var dist := pos.distance_to(a["pos"])
		if dist <= best_d:
			best_d = dist
			best = a
	return best


func get_animal(id: int) -> Dictionary:
	return animals.get(id, {})


## `by` killed this animal: it's gone, its kind gets warier, and its group moves off.
func kill(a: Dictionary, by: Colonist) -> void:
	animals.erase(a["id"])
	var kind := String(a["kind"])
	stats["kills"][kind] = int(stats["kills"].get(kind, 0)) + 1
	fear[kind] = float(fear.get(kind, 0.0)) + float(cfg().get("fear_per_kill", 0.15))
	var g: Dictionary = groups.get(a["group"], {})
	if not g.is_empty():
		var away: Vector2 = (Vector2(g["home"]) - by.pos).normalized()
		if away == Vector2.ZERO:
			away = Vector2.from_angle(rng.randf() * TAU)
		var h := sim.map.nearest_walkable(Vector2i((Vector2(g["home"]) + away * 8.0).round()).clamp(Vector2i.ONE, Vector2i(sim.map.size - 2, sim.map.size - 2)))
		if h.x >= 0:
			g["home"] = h
		for o: Dictionary in animals.values():
			if o["group"] == a["group"]:
				scare(o, by.pos)
	sim.events.animal_killed.emit(by, kind, Vector2i(a["pos"].round()))


func count(kind: String) -> int:
	var n := 0
	for a: Dictionary in animals.values():
		if a["kind"] == kind:
			n += 1
	return n


## ---- Spawning ------------------------------------------------------------------------

func _new_group(kind: String, home: Vector2i) -> int:
	var gid := _next_id
	_next_id += 1
	groups[gid] = {"id": gid, "kind": kind, "home": home,
		"drift_at": sim.clock + float(kind_def(kind).get("home_drift_days", 2.0)) * 1440.0 * rng.randf_range(0.5, 1.5)}
	return gid


func _spawn(kind: String, group: int, cell: Vector2i) -> Dictionary:
	if cell.x < 0:
		return {}
	var id := _next_id
	_next_id += 1
	var a := {"id": id, "kind": kind, "group": group, "pos": Vector2(cell), "target": Vector2(cell),
		"state": "idle", "timer": rng.randf_range(1.0, 20.0)}
	animals[id] = a
	return a


## A good home for a group: open ground away from the village (deer like the woods). With `near`
## set, somewhere within `within` tiles of it.
func _home_cell(kind: String, near: Vector2i, within: int = 0) -> Vector2i:
	var n := sim.map.size
	var woods := bool(kind_def(kind).get("near_trees", false))
	var village := Vector2(sim.map.center)
	for tries in 200:
		var c: Vector2i
		if near.x >= 0:
			c = near + Vector2i(rng.randi_range(-within, within), rng.randi_range(-within, within))
		else:
			c = Vector2i(rng.randi_range(2, n - 3), rng.randi_range(2, n - 3))
		if not sim.map.in_bounds(c) or not sim.map.is_walkable(c):
			continue
		var far := 12.0 if woods else 8.0
		if Vector2(c).distance_to(village) < far and tries < 150:
			continue
		if woods and tries < 150:
			var trees := 0
			for dy in range(-3, 4):
				for dx in range(-3, 4):
					var nd := sim.node_at_cell(c + Vector2i(dx, dy))
					if nd != null and nd.type == "tree":
						trees += 1
			if trees < 3:
				continue
		return c
	return Vector2i(-1, -1)


func _near_walkable(c: Vector2i, r: int) -> Vector2i:
	for tries in 20:
		var cc := c + Vector2i(rng.randi_range(-r, r), rng.randi_range(-r, r))
		if sim.map.in_bounds(cc) and sim.map.is_walkable(cc):
			return cc
	return sim.map.nearest_walkable(c) if sim.map.in_bounds(c) else Vector2i(-1, -1)


func _edge_cell() -> Vector2i:
	var n := sim.map.size
	for tries in 40:
		var t := rng.randi_range(1, n - 2)
		var c: Vector2i = [Vector2i(1, t), Vector2i(n - 2, t), Vector2i(t, 1), Vector2i(t, n - 2)][rng.randi() % 4]
		if sim.map.is_walkable(c):
			return c
	return Vector2i(-1, -1)


## ---- Saving --------------------------------------------------------------------------

func to_dict() -> Dictionary:
	return {"animals": animals.values().map(func(a: Dictionary) -> Dictionary: return a.duplicate()),
		"groups": groups.values().map(func(g: Dictionary) -> Dictionary: return g.duplicate()),
		"capacity": capacity.duplicate(), "fear": fear.duplicate(), "stats": stats.duplicate(true),
		"next_id": _next_id, "rng_state": rng.state}


func from_dict(d: Dictionary) -> void:
	if d.is_empty():
		return      # an older save: keep the animals setup() made for this map
	animals.clear()
	groups.clear()
	for a in d.get("animals", []):
		animals[int(a["id"])] = (a as Dictionary).duplicate()
	for g in d.get("groups", []):
		groups[int(g["id"])] = (g as Dictionary).duplicate()
	capacity = d.get("capacity", capacity).duplicate()
	fear = d.get("fear", fear).duplicate()
	stats = d.get("stats", stats).duplicate(true)
	_next_id = int(d.get("next_id", _next_id))
	rng.state = int(d.get("rng_state", rng.state))
