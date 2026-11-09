extends ActionBase
## Short break, near a campfire if one is close.
## params: minutes, energy_per_hour, optional warmth_per_hour, optional needs_fire (go to the
## nearest fire however far; used by "warm up by the fire").


func kind() -> String:
	return "rest"


func available() -> bool:
	if params.get("needs_fire", false):
		return sim.nearest_building_with("warmth", c.cell) != null
	return true


func begin() -> bool:
	var needs_fire: bool = params.get("needs_fire", false)
	var fire := sim.nearest_building_with("warmth" if needs_fire else "rest_spot", c.cell)
	if fire != null and (needs_fire or Vector2(fire.cell - c.cell).length() < 15.0):
		if not go_to(sim.map.access_cell(fire.cells(), c.cell)) and needs_fire:
			return false
	elif needs_fire:
		return false
	phase = "walking"
	timer = float(params.get("minutes", 30))
	return true


func describe() -> String:
	if params.get("needs_fire", false):
		return "warming up by the fire" if phase == "resting" else "going to the fire"
	return "resting"


func tick(dt: float) -> int:
	match phase:
		"walking":
			if walk(dt):
				phase = "resting"
		"resting":
			timer -= dt
			c.needs["energy"] = minf(100.0, float(c.needs["energy"]) + float(params.get("energy_per_hour", 4.0)) * dt / 60.0)
			var w := float(params.get("warmth_per_hour", 0.0))
			if w > 0.0 and c.needs.has("warmth"):
				c.needs["warmth"] = minf(100.0, float(c.needs["warmth"]) + w * dt / 60.0)
			if timer <= 0.0:
				return DONE
	return RUNNING
