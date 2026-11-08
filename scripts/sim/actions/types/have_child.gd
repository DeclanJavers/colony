extends ActionBase
## Go home with your partner in mind and start a pregnancy. params: minutes


func kind() -> String:
	return "socialize"


func available() -> bool:
	return sim.can_have_child(c)


func begin() -> bool:
	var house := sim.buildings.get(c.home_id) as Building
	if house == null or not house.complete:
		house = sim.nearest_building_with("beds", c.cell)
	if house == null or not go_to(sim.map.access_cell(house.cells(), c.cell)):
		return false
	phase = "walking"
	timer = float(params.get("minutes", 60))
	return true


func describe() -> String:
	return "starting a family"


func tick(dt: float) -> int:
	match phase:
		"walking":
			if walk(dt):
				phase = "at home"
		"at home":
			timer -= dt
			if timer <= 0.0:
				if not sim.can_have_child(c):
					return fail("conditions changed")
				sim.start_pregnancy(c)
				return DONE
	return RUNNING
