extends ActionBase
## Plant an empty farm plot or harvest a ripe one (harvest is carried to storage).

var farm: Building
var carrying: float = 0.0
var harvesting := false


func kind() -> String:
	return "work"


func available() -> bool:
	return sim.find_farm_job(c.cell) != null


func begin() -> bool:
	farm = sim.find_farm_job(c.cell)
	if farm == null or not go_to(sim.map.access_cell(farm.cells(), c.cell)):
		return false
	farm.reserved_by = c.id
	harvesting = farm.farm_stage == "ripe"
	phase = "walking"
	return true


func end(_ok: bool) -> void:
	if farm != null and farm.reserved_by == c.id:
		farm.reserved_by = -1


func describe() -> String:
	var what := "harvest" if harvesting else "plant"
	return "%s farm (%s)" % [what, phase] if phase != "" else "work the farm"


func tick(dt: float) -> int:
	var f: Dictionary = farm.provides("farm", {})
	match phase:
		"walking":
			if walk(dt):
				phase = "working"
				timer = float(f.get("harvest_minutes" if harvesting else "plant_minutes", 40))
		"working":
			timer -= dt * work_rate("farming")
			c.add_skill_xp("farming", dt)
			if timer <= 0.0:
				if harvesting:
					carrying = float(f.get("yield", 20)) * sim.colony_mult("food_yield")
					farm.farm_stage = "empty"
					farm.farm_growth = 0.0
					farm.harvests += 1
					if not go_to(sim.storage_access(c.cell)):
						return fail("no path to storage")
					phase = "carrying"
				else:
					farm.farm_stage = "growing"
					farm.farm_growth = 0.0
					return DONE
		"carrying":
			if walk(dt):
				sim.deposit("food", carrying)
				return DONE
	return RUNNING
