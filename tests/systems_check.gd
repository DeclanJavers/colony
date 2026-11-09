extends SceneTree
## Checks the colony's newer systems behave as designed: tools and litter, fishing, terrain and
## map options, roles, the weekly meeting and the hall, the story, saving mid-meeting, and
## game animals and hunting.
##   godot --headless --path . -s res://tests/systems_check.gd
## Prints one line per check and RESULT PASS / FAIL.

var fails := 0


func check(name: String, ok: bool, detail: String = "") -> void:
	print("%s  %s%s" % ["ok  " if ok else "FAIL", name, (" (" + detail + ")") if detail != "" else ""])
	if not ok:
		fails += 1


func _fresh(seed_v: int, options: Dictionary = {}) -> Sim:
	var sim := Sim.new()
	sim.register_brain(UtilityBrain.new(), true)
	sim.map_options = options
	sim.setup(seed_v)
	return sim


func _initialize() -> void:
	_tools_and_litter()
	_fishing()
	_terrain()
	_roles()
	_meetings_and_story()
	_hunting()
	print("RESULT %s" % ("PASS" if fails == 0 else "FAIL (%d)" % fails))
	quit(0 if fails == 0 else 1)


func _tools_and_litter() -> void:
	var sim := _fresh(1234)
	var c: Colonist = sim.colonists.values()[0]
	var litter := sim.nodes.values().filter(func(n: ResNode) -> bool: return not n.def().get("blocks", true))
	check("sticks and pebbles are scattered", litter.size() > 10, "%d" % litter.size())
	var walk_ok := litter.all(func(n: ResNode) -> bool: return sim.map.is_walkable(n.cell) and sim.map.litter_at.get(n.cell) == n.id)
	check("litter can be walked over", walk_ok)
	# Know every tree, then look for wood with and without a hatchet.
	for n: ResNode in sim.nodes.values():
		c.knowledge.see_place(n.id, n.type, n.cell, sim.clock, n.amount)
	var no_tool := sim.find_sources(["node:tree"], "wood", c.cell, 1.0, true, c)
	check("no hatchet: trees aren't a source", no_tool.is_empty(), "%d" % no_tool.size())
	sim.give_tool(c, "hatchet")
	var with_tool := sim.find_sources(["node:tree"], "wood", c.cell, 1.0, true, c)
	check("with a hatchet: trees are a source", not with_tool.is_empty())
	var uses := float(c.tools["hatchet"])
	for i in int(uses):
		sim.use_tool(c, "hatchet")
	check("a tool breaks when its uses run out", not c.has_tool("hatchet"))
	# Building over litter clears it.
	var lit: ResNode = litter[0]
	sim.stock["wood"] = 999.0
	var b := sim.place_site("stockpile", null, true, lit.cell - Vector2i(0, 0))
	check("building over sticks clears them", b != null and not sim.nodes.has(lit.id) and not sim.map.litter_at.has(lit.cell))


func _fishing() -> void:
	var sim := _fresh(1234)
	var c: Colonist = sim.colonists.values()[0]
	var fish_def: Dictionary = sim.content.actions_by_id["fish"]
	var act := sim.make_action(c, fish_def)
	check("no rod: can't fish", not act.available())
	sim.give_tool(c, "rod")
	c.knowledge.seen.fill(0)
	check("rod but no river seen: can't fish", not act.available())
	c.knowledge.seen.fill(1)
	check("rod and river seen: can fish", act.available())
	var shores := sim.seen_shores(c, 6)
	check("fishing spots are on the bank", not shores.is_empty() and sim.map.shore_cells.has(shores[0]))


func _terrain() -> void:
	var a := _fresh(4242)
	var b := _fresh(4242)
	check("same seed, same hills", a.map.height == b.map.height)
	var hilly := 0
	for v in a.map.height:
		hilly += 1 if v > 0 else 0
	check("there are hills", hilly > 50, "%d tiles" % hilly)
	var flat := _fresh(4242, {"legacy": true})
	var any := false
	for v in flat.map.height:
		any = any or v > 0
	check("old saves keep a flat map", not any)
	var ponds := _fresh(4242, {"ponds": 4})
	var water_a := 0
	var water_p := 0
	for i in a.map.tiles.size():
		water_a += 1 if a.map.tiles[i] == WorldMap.Tile.WATER else 0
		water_p += 1 if ponds.map.tiles[i] == WorldMap.Tile.WATER else 0
	check("more ponds, more water", water_p > water_a, "%d vs %d" % [water_p, water_a])
	# Buildings never straddle two heights.
	var straddle := false
	for i in 30:
		var spot := a.find_build_spot("house", a.map.center)
		if spot.x < 0:
			break
		var h := a.map.height_at(spot)
		for dy in 2:
			for dx in 2:
				straddle = straddle or a.map.height_at(spot + Vector2i(dx, dy)) != h
		a.place_site("house", null, true, spot)
	check("buildings stand on flat ground", not straddle)


func _roles() -> void:
	var sim := _fresh(1234)
	var people: Array = sim.colonists.values()
	var c: Colonist = people[0]
	c.skills["woodcutting"] = 4.0
	c.practice["woodcutting"] = 600.0
	c.practice["mining"] = 100.0
	sim._update_role_cuts()
	check("practised and best at it: a role", sim.role_for(c) == "woodcutting", sim.role_for(c))
	c.practice["mining"] = 590.0
	c.role = "woodcutting"
	check("a role sticks until another job clearly overtakes it", sim.role_for(c) == "woodcutting")
	for o: Colonist in people.slice(1):
		o.skills["woodcutting"] = 9.0
	sim._update_role_cuts()
	check("not among the colony's best: no role", sim.role_for(c) == "", sim.role_for(c))
	var kid := sim.spawn_colonist(sim.map.center, 0.5, [c.id])
	kid.skills["farming"] = 9.0
	kid.practice["farming"] = 900.0
	check("children have no role", sim.role_for(kid) == "")


func _meetings_and_story() -> void:
	var sim := _fresh(99)
	var held := []
	sim.events.meeting_held.connect(func(p: Dictionary) -> void: held.append(p))
	var weeks := []
	sim.events.week_summarized.connect(func(w: Dictionary) -> void: weeks.append(w))
	var saved_mid := {}
	# (Two weeks: a meeting nobody can come to, say everyone's asleep, isn't held; the next one is.)
	while sim.clock < 15.5 * 1440.0 and (held.is_empty() or saved_mid.is_empty()):
		sim.tick(1.0)
		if saved_mid.is_empty() and not sim.meetings.current.is_empty() and not bool(sim.meetings.current["talked"]):
			SaveGame.save(sim, [], "user://test_mid_meeting.tres")
			saved_mid = SaveGame.read("user://test_mid_meeting.tres")
	check("a meeting is held on a meeting day", not held.is_empty() and int(held[0]["day"]) % int(sim.content.meetings.get("every_days", 7)) == 0,
		"day %d" % int(held[0]["day"]) if not held.is_empty() else "none")
	if not held.is_empty():
		var p: Dictionary = held[0]
		check("people came", (p["present"] as Array).size() >= 2, "%d" % (p["present"] as Array).size())
		var once := true
		var seen := {}
		for it in p["items"]:
			for n in it.get("who", []):
				once = once and not seen.has(n)
				seen[n] = true
		check("each volunteer has one job", once)
		var knows := 0
		for c: Colonist in sim.colonists.values():
			knows += 1 if sim.meetings.knows_plan(c) else 0
		check("those there know the plan", knows >= (p["present"] as Array).size())
	check("the first week's story was written", not weeks.is_empty() and not (weeks[0]["lines"] as Array).is_empty())
	# Load the save made mid-meeting: the meeting carries on.
	if not saved_mid.is_empty():
		var b := Sim.new()
		b.register_brain(UtilityBrain.new(), true)
		b.map_options = saved_mid.get("map_options", {})
		b.setup(int(saved_mid["seed"]))
		SaveGame.apply(b, saved_mid)
		check("a save made mid-meeting keeps the meeting", not b.meetings.current.is_empty() and b.meetings.current["place"] != null)
	else:
		check("a save made mid-meeting keeps the meeting", false, "no meeting gathered")
	# The hall: enlarging when it was too crowded.
	var hall: Building = null
	for bb: Building in sim.buildings.values():
		if bb.type == "meeting_hall" and bb.complete:
			hall = bb
	if hall != null:
		sim.stock["wood"] = 999.0
		sim.stock["stone"] = 999.0
		var before := hall.seats()
		var ok := sim.can_expand(hall)
		var e := sim.start_expansion("meeting_hall", null) if ok else null
		if e != null:
			sim.complete_building(e, null)
		var cells_ok := true
		for cell in hall.cells():
			cells_ok = cells_ok and sim.map.building_at.get(cell, -1) == hall.id
		check("the hall can be enlarged and seats more", e != null and hall.seats() > before and cells_ok, "%d -> %d" % [before, hall.seats()])
	else:
		check("the hall can be enlarged and seats more", false, "no hall yet")


func _hunting() -> void:
	var sim := _fresh(1234)
	var w := sim.wildlife
	check("rabbits and deer live on the land", w.count("rabbit") >= 6 and w.count("deer") >= 4, "%d rabbits, %d deer" % [w.count("rabbit"), w.count("deer")])
	check("they stand on open ground", w.animals.values().all(func(a: Dictionary) -> bool: return sim.map.is_walkable(Vector2i(a["pos"].round()))))
	var few := _fresh(1234, {"game": 0.5})
	check("fewer deer and rabbits when asked", few.wildlife.animals.size() < w.animals.size(), "%d vs %d" % [few.wildlife.animals.size(), w.animals.size()])
	# A deer bolts from someone walking up; a hunter creeping up gets closer first.
	var c: Colonist = sim.colonists.values()[0]
	var deer: Dictionary = w.animals.values().filter(func(a: Dictionary) -> bool: return a["kind"] == "deer")[0]
	var spot := Vector2(sim.map.nearest_walkable(Vector2i((deer["pos"] + Vector2(3.5, 0)).round())))
	c.pos = spot
	c.cell = Vector2i(spot)
	deer["state"] = "idle"
	deer["timer"] = 999.0
	w.update(1.0)
	check("a deer bolts when someone comes close", deer["state"] == "flee")
	deer["state"] = "idle"
	deer["timer"] = 999.0
	c.set_meta("stalking", true)
	w.update(1.0)
	check("a hunter creeping up gets closer before it bolts", deer["state"] == "idle")
	c.set_meta("stalking", false)
	# Hunting end to end: a spear, a sighting, and off they go.
	sim.give_tool(c, "spear")
	c.knowledge.game.clear()
	var hunt := sim.make_action(c, sim.content.actions_by_id["hunt"])
	check("no sighting: can't hunt", not hunt.available())
	var food0 := float(sim.stock["food"])
	var kills0 := w.animals.size()
	var got := false
	for attempt in 12:
		var rab: Array = w.animals.values().filter(func(a: Dictionary) -> bool: return a["kind"] == "rabbit")
		if rab.is_empty():
			break
		var r: Dictionary = rab[attempt % rab.size()]
		c.knowledge.see_game("rabbit", Vector2i(r["pos"].round()), sim.clock, 1)
		var act := sim.make_action(c, sim.content.actions_by_id["hunt"])
		if not act.available() or not act.begin():
			continue
		for m in 400:
			sim.clock += 1.0
			w.update(1.0)
			if act.tick(1.0) != ActionBase.RUNNING:
				break
		act.end(true)
		if float(sim.stock["food"]) > food0:
			got = true
			break
	check("a hunter can bring meat home", got, "food %d -> %d" % [food0, sim.stock["food"]])
	check("the hunted animal is gone", w.animals.size() < kills0 or not got)
	# Over-hunting: kill every deer. They grow wary, and in time a few move back in.
	var hunter: Colonist = sim.colonists.values()[1]
	for a: Dictionary in w.animals.values().filter(func(x: Dictionary) -> bool: return x["kind"] == "deer"):
		w.kill(a, hunter)
	check("hunting deer out leaves none", w.count("deer") == 0)
	check("hunted animals grow warier", float(w.fear["deer"]) > 0.3, "%.2f" % float(w.fear["deer"]))
	for d in 40:
		sim.clock += 1440.0
		w.on_day()
	check("hunted-out land recovers in time", w.count("deer") > 0, "%d deer after 40 days" % w.count("deer"))
	check("the fear fades", float(w.fear["deer"]) < 0.05)
	# What people know: they see game, tell each other, and Laya's state text mentions it.
	var other: Colonist = sim.colonists.values()[2]
	var any_a: Dictionary = w.animals.values()[0]
	hunter.pos = any_a["pos"] + Vector2(1, 0)
	hunter.cell = Vector2i(hunter.pos.round())
	hunter.knowledge.game.clear()
	sim._see_game(hunter)
	check("people remember game they see", not hunter.knowledge.fresh_game(sim.clock).is_empty())
	other.knowledge.game.clear()
	for k in 10:
		sim.share_game(hunter, other)
	check("sightings are passed on in talk", not other.knowledge.fresh_game(sim.clock).is_empty())
	check("the state text mentions game", sim.describer.describe(sim, hunter).contains("Game: "))
	# Saving keeps the animals where they are.
	SaveGame.save(sim, [], "user://test_wildlife.tres")
	var data := SaveGame.read("user://test_wildlife.tres")
	var b := Sim.new()
	b.register_brain(UtilityBrain.new(), true)
	b.map_options = data.get("map_options", {})
	b.setup(int(data["seed"]))
	SaveGame.apply(b, data)
	var same := b.wildlife.animals.size() == w.animals.size()
	for id in w.animals:
		same = same and b.wildlife.animals.has(id) and (b.wildlife.animals[id]["pos"] as Vector2).is_equal_approx(w.animals[id]["pos"])
	check("saving keeps the animals", same, "%d vs %d" % [b.wildlife.animals.size(), w.animals.size()])
	check("saving keeps what people saw", not b.find_colonist(other.id).knowledge.fresh_game(b.clock).is_empty())

