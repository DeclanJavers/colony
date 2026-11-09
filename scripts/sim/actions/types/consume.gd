extends ActionBase
## Eat / drink. params: need, resource, use, restore, minutes, sources[]

var source: Dictionary = {}


func kind() -> String:
	return "need"


func available() -> bool:
	return not sim.find_sources(params["sources"], params["resource"], c.cell, float(params.get("use", 1)), false, c).is_empty()


func begin() -> bool:
	for s in sim.find_sources(params["sources"], params["resource"], c.cell, float(params.get("use", 1)), false, c):
		if go_to(s["access"]):
			source = s
			phase = "walking"
			return true
		if s.get("kind") == "node":
			(s["ref"] as ResNode).unreachable_until = sim.clock + 12.0 * 60.0
			c.knowledge.block_place((s["ref"] as ResNode).id, sim.clock + 12.0 * 60.0)   # cut off: skip it for a while
	return false


func tick(dt: float) -> int:
	match phase:
		"walking":
			if walk(dt):
				var ok := sim.take_from_source(source, params["resource"], float(params.get("use", 1)))
				sim.saw_node_after_use(c, source)
				if source.get("kind") == "stockpile":
					sim.observe_stock(c)   # at the stores: they see what's (still) there
				if not ok:
					return fail("source ran out")
				phase = "using"
				timer = float(params.get("minutes", 10))
		"using":
			timer -= dt
			if timer <= 0.0:
				var need: String = params["need"]
				c.needs[need] = minf(100.0, float(c.needs[need]) + float(params.get("restore", 50)))
				return DONE
	return RUNNING
