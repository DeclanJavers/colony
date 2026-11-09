class_name SaveGame
extends RefCounted
## Save and load a whole colony. The map is rebuilt from the seed (it's deterministic); everything
## that changes over time is stored: clock, stock, tier, stats, resource nodes, buildings (with
## their history), colonists (needs, beliefs, memories, requests, conversations, relationships,
## family) and the graveyard. Colonists' current actions aren't stored: after loading, everyone
## simply decides again what to do next.

const PATH := "user://save.tres"
const FORMAT := 1

static var pending: Dictionary = {}   # set before reloading the scene; main.gd starts from it


static func save(sim: Sim, history: Array, path: String = PATH, chronicle: Array = []) -> Error:
	var data := {
		"format": FORMAT, "seed": sim.seed_value, "clock": sim.clock, "stock": sim.stock.duplicate(),
		"tier": sim.unlocked_tier, "stats": sim.stats.duplicate(true), "next_id": sim._next_id,
		"next_request_id": sim._next_request_id, "used_names": sim._used_names.duplicate(),
		"conv_id": sim._conv_id, "weather": sim.weather, "footfall": sim.footfall, "discovered": sim.discovered, "rng_state": sim.rng.state, "history": history.duplicate(true), "chronicle": chronicle.duplicate(true),
		"nodes": [], "buildings": [], "colonists": [], "graveyard": [],
	}
	for n: ResNode in sim.nodes.values():
		data["nodes"].append({"id": n.id, "type": n.type, "cell": n.cell, "amount": n.amount, "max": n.max_amount})
	for b: Building in sim.buildings.values():
		data["buildings"].append({"id": b.id, "type": b.type, "cell": b.cell, "size": b.size, "complete": b.complete,
			"work_done": b.work_done, "work_needed": b.work_needed, "beds": b.beds.duplicate(),
			"farm_stage": b.farm_stage, "farm_growth": b.farm_growth, "placed_at": b.placed_at,
			"completed_at": b.completed_at, "placed_by": b.placed_by, "work_by": b.work_by.duplicate(),
			"harvests": b.harvests})
	for c: Colonist in sim.colonists.values():
		data["colonists"].append(_colonist(c))
	for g in sim.graveyard:
		data["graveyard"].append(_colonist(g))
	var f := FileAccess.open(path, FileAccess.WRITE)
	if f == null:
		return FileAccess.get_open_error()
	f.store_string(var_to_str(data))
	return OK


static func read(path: String = PATH) -> Dictionary:
	if not FileAccess.file_exists(path):
		return {}
	var v = str_to_var(FileAccess.get_file_as_string(path))
	return v if typeof(v) == TYPE_DICTIONARY and int(v.get("format", 0)) == FORMAT else {}


static func exists(path: String = PATH) -> bool:
	return FileAccess.file_exists(path)


## Replace a freshly set-up sim's state (same seed) with the saved one.
static func apply(sim: Sim, data: Dictionary) -> void:
	sim.clock = float(data["clock"])
	sim.stock = data["stock"].duplicate()
	sim.unlocked_tier = int(data["tier"])
	sim.stats = data["stats"].duplicate(true)
	sim._next_id = int(data["next_id"])
	sim._next_request_id = int(data["next_request_id"])
	sim._used_names = data["used_names"].duplicate()
	sim._conv_id = int(data.get("conv_id", 0))
	sim.weather = String(data.get("weather", "clear"))
	var ff: Variant = data.get("footfall")
	if ff is PackedFloat32Array and (ff as PackedFloat32Array).size() == sim.footfall.size():
		sim.footfall = ff
	sim.rng.state = int(data["rng_state"])
	sim._last_hour = sim.hour()
	sim._last_day = sim.day()
	sim._last_season = sim.season()
	# Clear what setup() created, then restore.
	var touched := {}
	for cell in sim.map.node_at.keys() + sim.map.building_at.keys():
		touched[cell] = true
	sim.map.node_at.clear()
	sim.map.building_at.clear()
	sim.nodes.clear()
	sim.buildings.clear()
	sim.colonists.clear()
	sim.graveyard.clear()
	for d in data["nodes"]:
		var n := ResNode.new()
		n.id = int(d["id"]); n.type = d["type"]; n.cell = d["cell"]
		n.amount = float(d["amount"]); n.max_amount = float(d["max"])
		sim.nodes[n.id] = n
		sim.map.node_at[n.cell] = n.id
		touched[n.cell] = true
	for d in data["buildings"]:
		var b := Building.new()
		b.id = int(d["id"]); b.type = d["type"]; b.cell = d["cell"]; b.size = d["size"]
		b.complete = bool(d["complete"]); b.work_done = float(d["work_done"]); b.work_needed = float(d["work_needed"])
		b.beds = d["beds"].duplicate(); b.farm_stage = d["farm_stage"]; b.farm_growth = float(d["farm_growth"])
		b.placed_at = float(d["placed_at"]); b.completed_at = float(d["completed_at"]); b.placed_by = d["placed_by"]
		b.work_by = d["work_by"].duplicate(); b.harvests = int(d["harvests"])
		sim.buildings[b.id] = b
		if not b.def().get("walkable", false):
			for cell in b.cells():
				sim.map.building_at[cell] = b.id
				touched[cell] = true
	for cell in touched:
		sim.map.refresh_cell(cell)
	for d in data["colonists"]:
		var c := _restore(d, sim)
		sim.colonists[c.id] = c
	for d in data["graveyard"]:
		sim.graveyard.append(_restore(d, sim))
	# What the colony has seen. Saves from before exploration: everyone knows the camp's surroundings.
	var disc: Variant = data.get("discovered")
	sim.discovered.fill(0)
	sim.discovered_count = 0
	var disc_ok := disc is PackedByteArray and (disc as PackedByteArray).size() == sim.discovered.size()
	if disc_ok:
		sim.discovered = disc
		sim.discovered_count = sim.discovered.count(1)
	for c: Colonist in sim.colonists.values():
		if not disc_ok or c.knowledge.places.is_empty() or c.knowledge.seen.size() != sim.discovered.size():
			sim.look_around(c, Sim.FOUNDERS_KNOW, Vector2(sim.map.center))
		c.remove_meta("looked_from")


static func _colonist(c: Colonist) -> Dictionary:
	return {"id": c.id, "name": c.name, "age": c.age_minutes, "lifespan": c.lifespan_days, "traits": Array(c.traits),
		"needs": c.needs.duplicate(), "zero": c.zero_minutes.duplicate(), "skills": c.skills.duplicate(),
		"memories": c.memories.duplicate(true), "recent": c.recent.duplicate(true),
		"knowledge": c.knowledge.facts.duplicate(true), "places": c.knowledge.places.duplicate(true), "seen": c.knowledge.seen, "requests": c.requests.duplicate(true),
		"conversations": c.conversations.duplicate(true), "affection": c.affection.duplicate(),
		"partner": c.partner_id, "parents": c.parent_ids.duplicate(), "home": c.home_id, "pos": c.pos, "cell": c.cell,
		"dead": not c.alive(), "brain": c.brain_id, "pregnant_until": c.pregnant_until, "last_child_at": c.last_child_at,
		"cause": c.cause_of_death, "died_at": c.died_at}


static func _restore(d: Dictionary, sim: Sim) -> Colonist:
	var c := Colonist.new()
	c.id = int(d["id"]); c.name = d["name"]; c.age_minutes = float(d["age"]); c.lifespan_days = float(d["lifespan"])
	c.traits.assign(d["traits"])
	c.needs = d["needs"].duplicate(); c.zero_minutes = d["zero"].duplicate(); c.skills = d["skills"].duplicate()
	c.memories = d["memories"].duplicate(true); c.recent = d["recent"].duplicate(true)
	c.knowledge = Knowledge.new(c.id)
	c.knowledge.facts = d["knowledge"].duplicate(true)
	c.knowledge.places = d.get("places", {}).duplicate(true)
	var seen: Variant = d.get("seen")
	if seen is PackedByteArray:
		c.knowledge.seen = seen
	c.requests = d["requests"].duplicate(true); c.conversations = d["conversations"].duplicate(true)
	c.affection = d["affection"].duplicate(); c.partner_id = int(d["partner"]); c.parent_ids = d["parents"].duplicate()
	c.home_id = int(d["home"]); c.pos = d["pos"]; c.cell = d["cell"]; c.brain_id = d["brain"]
	c.pregnant_until = float(d["pregnant_until"]); c.last_child_at = float(d["last_child_at"])
	c.cause_of_death = d["cause"]; c.died_at = float(d["died_at"])
	c.state = "dead" if bool(d["dead"]) else "idle"
	c.hesitate_until = sim.clock
	return c
