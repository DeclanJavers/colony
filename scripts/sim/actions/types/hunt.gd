extends ActionBase
## Hunt rabbits or deer with a spear (data/animals.json). Go to where game was last seen (by them
## or someone who told them), look about; once an animal is in sight, creep up on it (slowly, so it
## doesn't bolt as early) and throw from close enough. A hit gives meat to carry home; a miss sends
## it running, and they can try again a couple of times. Better hunters get closer and hit more.
## If the animals have moved on, they give up and forget that sighting. An animal that bolts just
## as they get close gets a hurried throw (worse odds). Animals they can't get to (across water,
## boxed in) are let be.
## params: resource, minutes, throw_range, stalk_speed, throws, search_minutes, skill, tool

var prey_kind := ""            # what they're stalking now
var sought := ""               # the sighting they set out on (forgotten if it comes to nothing)
var _unreachable := {}         # animal ids they found no way to
var _tool_used := false
var prey_id := -1
var carrying := 0.0
var throws := 0
var _search := 0.0
var _look := 0.0


func kind() -> String:
	return "gather"


func available() -> bool:
	return c.has_tool(String(params.get("tool", "spear"))) and sim.has_storage_space() \
		and not c.knowledge.fresh_game(sim.clock).is_empty()


## The sighting worth going for: more meat and nearer and fresher is better.
func _pick_sighting() -> Dictionary:
	var best := {}
	var best_s := -INF
	for e in c.knowledge.fresh_game(sim.clock):
		var food := float(sim.wildlife.kind_def(e["kind"]).get("food", 5))
		var dist := Vector2(e["cell"] - c.cell).length()
		var s := food / (dist + 8.0) / (1.0 + float(e["age_h"]) / 12.0)
		if s > best_s:
			best_s = s
			best = e
	return best


func begin() -> bool:
	var e := _pick_sighting()
	if e.is_empty():
		return false
	prey_kind = String(e["kind"])
	sought = prey_kind
	if not go_to(sim.map.nearest_walkable(e["cell"]), false):
		c.knowledge.forget_game(prey_kind)      # can't get there (across the river, walled in)
		return false
	phase = "tracking"
	timer = float(params.get("minutes", 120))
	sim.wildlife.stats["hunts"] = int(sim.wildlife.stats["hunts"]) + 1
	return true


func describe() -> String:
	var label := String(sim.wildlife.kind_def(prey_kind).get("plural", prey_kind)) if prey_kind != "" else "game"
	match phase:
		"tracking", "searching":
			return "looking for %s" % label
		"stalking":
			return "stalking a %s" % sim.wildlife.kind_def(prey_kind).get("label", prey_kind)
		"carrying":
			return "bringing the meat home"
	return "going hunting"


func end(_ok: bool) -> void:
	c.set_meta("stalking", false)
	if carrying > 0.0:
		_store()     # called away on the way home (the meeting bell): they drop the meat at the stores


func _store() -> void:
	var label := String(sim.wildlife.kind_def(prey_kind).get("label", prey_kind))
	var kept := sim.deposit(String(params.get("resource", "food")), carrying, c)
	carrying = 0.0
	if kept > 0.0:
		sim.add_memory(c, "I hunted a %s and brought the meat home." % label, 2.0)
	else:
		sim.add_memory(c, "I hunted a %s, but the stores were full and the meat was wasted." % label, 2.0)


func tick(dt: float) -> int:
	var skill := String(params.get("skill", "hunting"))
	if phase != "carrying":
		timer -= dt
		c.add_skill_xp(skill, dt)
		if timer <= 0.0:
			if throws == 0:
				c.knowledge.forget_game(sought)    # nothing worth a throw out there
			return _give_up("I went hunting but came back with nothing.")
	match phase:
		"tracking":
			if _spot_prey():
				return RUNNING
			if walk(dt):
				phase = "searching"
				_search = float(params.get("search_minutes", 25))
		"searching":
			# Not where they were seen: cast about a little.
			_search -= dt
			if _spot_prey():
				return RUNNING
			if _search <= 0.0:
				c.knowledge.forget_game(sought)
				return _give_up("I went looking for %s but they'd moved on." % sim.wildlife.kind_def(sought).get("plural", sought))
			if walk(dt):
				var r := Vector2i(sim.rng.randi_range(-4, 4), sim.rng.randi_range(-4, 4))
				go_to(sim.map.nearest_walkable(c.cell + r), false)
		"stalking":
			var a := sim.wildlife.get_animal(prey_id)
			if a.is_empty():
				return _back_to_search()
			var dist := c.pos.distance_to(a["pos"])
			if dist > Sim.LOOK + 2.0:
				return _back_to_search()    # lost it
			var reach := float(params.get("throw_range", 2.5)) * (1.0 + 0.5 * (c.skill_mult(skill) - 1.0))
			if dist <= reach:
				return _throw(a, skill, 1.0)
			if a["state"] == "flee" and dist <= reach * 1.6:
				return _throw(a, skill, 0.5)   # it's off: a hurried throw as it goes
			# Creep closer: re-aim at where it is now.
			_look -= dt
			if _look <= 0.0 or path.is_empty():
				_look = 3.0
				# No way to it, or only a long way round (water, walls): you can't creep up on
				# something that far about. Leave it be.
				if not go_to(Vector2i(a["pos"].round()), false) or path.size() > dist * 2.0 + 4.0:
					_unreachable[prey_id] = true
					return _back_to_search()
			walk(dt * float(params.get("stalk_speed", 0.6)))
		"carrying":
			if walk(dt):
				_store()
				return DONE
	return RUNNING


## Any animal of the kind they're after (or any game, if that one's gone) in sight?
func _spot_prey() -> bool:
	var a := sim.wildlife.nearest(c.pos, Sim.LOOK, [prey_kind], _unreachable)
	if a.is_empty():
		a = sim.wildlife.nearest(c.pos, Sim.LOOK, [], _unreachable)
	if a.is_empty():
		return false
	prey_id = int(a["id"])
	prey_kind = String(a["kind"])
	phase = "stalking"
	c.set_meta("stalking", true)
	_look = 0.0
	return true


func _back_to_search() -> int:
	c.set_meta("stalking", false)
	phase = "searching"
	_search = float(params.get("search_minutes", 25)) * 0.5
	path.clear()
	return RUNNING


func _throw(a: Dictionary, skill: String, odds: float) -> int:
	throws += 1
	var d := sim.wildlife.kind_def(String(a["kind"]))
	var hit := float(d.get("hit_chance", 0.4)) * c.skill_mult(skill) * sim.night_mult() * odds
	c.set_meta("stalking", false)
	if sim.rng.randf() < hit:
		sim.wildlife.kill(a, c)
		carrying = float(d.get("food", 5))
		_wear_spear()
		phase = "carrying"
		if not go_to(sim.storage_access(c.cell)):
			_store()          # (no path home: drop it at the stores anyway, as end() would)
			return DONE
		return RUNNING
	sim.wildlife.stats["misses"] = int(sim.wildlife.stats["misses"]) + 1
	sim.events.hunt_missed.emit(c, String(a["kind"]), Vector2i(a["pos"].round()))
	sim.wildlife.scare(a, c.pos)
	if throws >= int(params.get("throws", 3)):
		return _give_up("I threw my spear at a %s and missed; it got away." % d.get("label", a["kind"]))
	return _back_to_search()


func _give_up(memory: String) -> int:
	c.set_meta("stalking", false)
	if throws > 0:
		_wear_spear()
	sim.add_memory(c, memory, 1.0)
	return DONE


## A trip where the spear was thrown wears it once (like a fishing trip wears the rod).
func _wear_spear() -> void:
	if not _tool_used:
		_tool_used = true
		sim.use_tool(c, String(params.get("tool", "spear")))
