extends ActionBase
## Eat / drink. params: need, resource, use, restore, minutes, sources[]

var source: Dictionary = {}


func kind() -> String:
	return "need"


func available() -> bool:
	return not sim.find_sources(params["sources"], params["resource"], c.cell, float(params.get("use", 1))).is_empty()


func begin() -> bool:
	for s in sim.find_sources(params["sources"], params["resource"], c.cell, float(params.get("use", 1))):
		if go_to(s["access"]):
			source = s
			phase = "walking"
			return true
	return false


func tick(dt: float) -> int:
	match phase:
		"walking":
			if walk(dt):
				if not sim.take_from_source(source, params["resource"], float(params.get("use", 1))):
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
