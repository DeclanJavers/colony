extends ActionBase
## Collect a resource from a source and carry it to shared storage.
## params: resource, sources[], optional per_trip, work_minutes

var source: Dictionary = {}
var carrying: float = 0.0
var skill := "gathering"


func kind() -> String:
	return "gather"


func available() -> bool:
	return sim.has_storage_space() and not sim.find_sources(params["sources"], params["resource"], c.cell, 1.0, true, c).is_empty()


func begin() -> bool:
	for s in sim.find_sources(params["sources"], params["resource"], c.cell, 1.0, true, c):
		if go_to(s["access"]):
			source = s
			sim.reserve_source(source, 1)
			phase = "walking"
			return true
		if s.get("kind") == "node":
			# No way there (cut off by water or buildings): don't keep offering it for a while.
			(s["ref"] as ResNode).unreachable_until = sim.clock + 12.0 * 60.0
	return false


func end(_ok: bool) -> void:
	sim.reserve_source(source, -1)


func tick(dt: float) -> int:
	match phase:
		"walking":
			if walk(dt):
				phase = "working"
				timer = float(params.get("work_minutes", sim.source_def(source).get("work_minutes", 20)))
		"working":
			timer -= dt * work_rate(skill)
			c.add_skill_xp(skill, dt)
			if timer <= 0.0:
				var per_trip := float(params.get("per_trip", sim.source_def(source).get("per_trip", 5)))
				per_trip *= c.trait_mod("carry")
				carrying = sim.harvest_source(source, per_trip)
				sim.saw_node_after_use(c, source)
				if carrying <= 0.0:
					return fail("source empty")
				if not go_to(sim.storage_access(c.cell)):
					return fail("no path to storage")
				phase = "carrying"
		"carrying":
			if walk(dt):
				sim.deposit(params["resource"], carrying, c)
				return DONE
	return RUNNING
