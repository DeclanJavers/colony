extends ActionBase
## Short break, near a campfire if one is close. params: minutes, energy_per_hour


func kind() -> String:
	return "rest"


func begin() -> bool:
	var fire := sim.nearest_building_with("rest_spot", c.cell)
	if fire != null and Vector2(fire.cell - c.cell).length() < 15.0:
		go_to(sim.map.access_cell(fire.cells(), c.cell))
	phase = "walking"
	timer = float(params.get("minutes", 30))
	return true


func describe() -> String:
	return "resting"


func tick(dt: float) -> int:
	match phase:
		"walking":
			if walk(dt):
				phase = "resting"
		"resting":
			timer -= dt
			c.needs["energy"] = minf(100.0, float(c.needs["energy"]) + float(params.get("energy_per_hour", 4.0)) * dt / 60.0)
			if timer <= 0.0:
				return DONE
	return RUNNING
