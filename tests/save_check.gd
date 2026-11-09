extends SceneTree
## Save → load round trip.  godot --headless --path . -s res://tests/save_check.gd

var fails := 0


func check(name: String, ok: bool, detail: String = "") -> void:
	print("%s  %s%s" % ["ok  " if ok else "FAIL", name, (" (" + detail + ")") if detail != "" else ""])
	if not ok:
		fails += 1


## Same keys and values within float rounding (the text save format trims long decimals).
func _close(p: Dictionary, q: Dictionary) -> bool:
	if p.keys().size() != q.keys().size():
		return false
	for k in p:
		if not q.has(k) or absf(float(p[k]) - float(q[k])) > 0.001:
			return false
	return true


## Belief facts match: same keys, sources and times, values within float rounding.
func _same_facts(p: Dictionary, q: Dictionary) -> bool:
	if p.keys().size() != q.keys().size():
		return false
	for k in p:
		if not q.has(k):
			return false
		var x: Dictionary = p[k]
		var y: Dictionary = q[k]
		if x["src"] != y["src"] or x["from"] != y["from"] or absf(float(x["t"]) - float(y["t"])) > 0.001 \
				or absf(float(x["v"]) - float(y["v"])) > 0.001:
			return false
	return true


func _initialize() -> void:
	var a := Sim.new()
	a.register_brain(UtilityBrain.new(), true)
	a.setup(1234)
	for i in 3 * 1440:
		a.tick(1.0)
	var path := "user://test_save.tres"
	check("saves", SaveGame.save(a, [], path) == OK)
	var data := SaveGame.read(path)
	check("reads back", not data.is_empty())

	var b := Sim.new()
	b.register_brain(UtilityBrain.new(), true)
	b.setup(int(data["seed"]))
	SaveGame.apply(b, data)
	check("same summary", JSON.stringify(a.summary()) == JSON.stringify(b.summary()))
	check("same clock and stock", absf(a.clock - b.clock) < 0.001 and _close(a.stock, b.stock))
	var same_people := a.colonists.size() == b.colonists.size()
	var detail := ""
	for id in a.colonists:
		var x: Colonist = a.colonists[id]
		var y: Colonist = b.colonists.get(id)
		if y == null or x.name != y.name or not _same_facts(x.knowledge.facts, y.knowledge.facts) or x.memories.size() != y.memories.size() \
				or not _close(x.affection, y.affection) or x.home_id != y.home_id or x.pos != y.pos:
			same_people = false
			detail = "%s: facts %s mem %s aff %s home %s pos %s" % [x.name, _same_facts(x.knowledge.facts, y.knowledge.facts),
				x.memories.size() == y.memories.size(), _close(x.affection, y.affection), x.home_id == y.home_id, x.pos == y.pos]
			if not _same_facts(x.knowledge.facts, y.knowledge.facts):
				detail += " | %s vs %s" % [x.knowledge.facts, y.knowledge.facts]
	check("same colonists (beliefs, memories, feelings, homes)", same_people, detail)
	var walk_ok := true
	for bl: Building in a.buildings.values():
		for cell in bl.cells():
			if a.map.is_walkable(cell) != b.map.is_walkable(cell):
				walk_ok = false
	check("buildings block the same tiles", walk_ok and a.buildings.size() == b.buildings.size())
	check("same resource nodes", a.nodes.size() == b.nodes.size())
	check("same explored land", a.discovered == b.discovered and a.discovered_count == b.discovered_count)
	var places_ok := true
	for cid in a.colonists:
		var x: Colonist = a.colonists[cid]
		var y: Colonist = b.colonists.get(cid)
		if y == null or x.knowledge.places.size() != y.knowledge.places.size() or x.knowledge.seen != y.knowledge.seen:
			places_ok = false
	check("same known places and seen land", places_ok)
	check("same footpaths", a.footfall == b.footfall)
	for i in 2 * 1440:
		b.tick(1.0)
	check("loaded colony keeps going", b.alive_count() > 0 and int(b.stats["failed_decisions"]) == 0,
		"pop %d, failed %d" % [b.alive_count(), int(b.stats["failed_decisions"])])
	DirAccess.remove_absolute(ProjectSettings.globalize_path(path))
	print("RESULT ", "PASS" if fails == 0 else "FAIL (%d)" % fails)
	quit(0 if fails == 0 else 1)
