extends ActionBase
## Fish from the river bank (needs a rod): walk to a bit of shore they've seen, fish for a while
## with a chance of a catch every few minutes (better with practice, poor in winter), then carry
## the catch to the stores. params: resource, minutes, catch_every, catch_chance, per_catch,
## skill, tool

var carrying := 0.0
var spot := Vector2i(-1, -1)
var _next_bite := 0.0


func kind() -> String:
	return "gather"


func available() -> bool:
	return c.has_tool(String(params.get("tool", "rod"))) and sim.has_storage_space() and _shore().x >= 0


func _shore() -> Vector2i:
	return sim.seen_shore(c)


func begin() -> bool:
	# The nearest bit of bank they've seen that they can actually walk to (not across the river).
	for s: Vector2i in sim.seen_shores(c, 6):
		if go_to(s, false):
			spot = s
			phase = "walking"
			return true
	return false


func describe() -> String:
	match phase:
		"fishing":
			return "fishing (%d caught)" % int(carrying / float(params.get("per_catch", 4)))
		"carrying":
			return "bringing the catch home"
	return "going fishing"


func tick(dt: float) -> int:
	match phase:
		"walking":
			if walk(dt):
				phase = "fishing"
				timer = float(params.get("minutes", 60))
				_next_bite = float(params.get("catch_every", 12))
		"fishing":
			timer -= dt
			_next_bite -= dt
			var skill := String(params.get("skill", "fishing"))
			c.add_skill_xp(skill, dt)
			if _next_bite <= 0.0:
				_next_bite = float(params.get("catch_every", 12))
				var chance := float(params.get("catch_chance", 0.3)) * c.skill_mult(skill) * float(sim.season_def().get("fish", 1.0))
				if sim.rng.randf() < chance:
					carrying += float(params.get("per_catch", 4))
					sim.events.fish_caught.emit(c, spot)
			if timer <= 0.0:
				sim.use_tool(c, String(params.get("tool", "rod")))
				if carrying <= 0.0:
					sim.add_memory(c, "I went fishing and caught nothing.", 1.0)
					return DONE
				if not go_to(sim.storage_access(c.cell)):
					return fail("no path to storage")
				phase = "carrying"
		"carrying":
			if walk(dt):
				sim.deposit(String(params.get("resource", "food")), carrying, c)
				sim.add_memory(c, "I caught %d fish." % int(carrying / float(params.get("per_catch", 4))), 1.5)
				return DONE
	return RUNNING
