extends ActionBase
## Sleep until rested. Best in your house bed, OK by a campfire, worst on the ground.
## params: base_rate_per_hour, wake_at

var quality: float = 1.0
var where: String = "on the ground"


func kind() -> String:
	return "rest"


func begin() -> bool:
	var home := sim.buildings.get(c.home_id) as Building
	if home != null and home.complete and go_to(sim.map.access_cell(home.cells(), c.cell)):
		quality = float(home.provides("sleep_quality", 1.5))
		where = "at home"
	else:
		var fire := sim.nearest_building_with("rest_spot", c.cell)
		if fire != null and go_to(sim.map.access_cell(fire.cells(), c.cell)):
			quality = float(fire.provides("rest_spot", 1.2))
			where = "by the %s" % fire.def().get("label", fire.type)
		else:
			path.clear()
	phase = "walking"
	return true


func need_decay_mult(need: String) -> float:
	if phase == "sleeping":
		return 0.0 if need == "energy" else 0.5
	return 1.0


func describe() -> String:
	return "sleeping %s" % where if phase == "sleeping" else "going to sleep"


func tick(dt: float) -> int:
	match phase:
		"walking":
			if walk(dt):
				phase = "sleeping"
		"sleeping":
			var night_bonus := 1.2 if sim.is_night() else 1.0
			c.needs["energy"] = minf(100.0, float(c.needs["energy"]) + float(params.get("base_rate_per_hour", 9.0)) * quality * night_bonus * dt / 60.0)
			if float(c.needs["energy"]) >= float(params.get("wake_at", 97)):
				return DONE
	return RUNNING
