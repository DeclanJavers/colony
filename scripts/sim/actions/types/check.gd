extends ActionBase
## Go to the shared stores and look at what's there. Colonists only know the stock from what
## they've seen or been told, so checking is how they get fresh, exact numbers.
## params: minutes


func kind() -> String:
	return "work"


func available() -> bool:
	return sim.storage_capacity() > 0.0


func begin() -> bool:
	if not go_to(sim.storage_access(c.cell)):
		return false
	phase = "walking"
	return true


func describe() -> String:
	return "checking the stores" if phase == "looking" else "going to check the stores"


func tick(dt: float) -> int:
	match phase:
		"walking":
			if walk(dt):
				phase = "looking"
				timer = float(params.get("minutes", 5))
		"looking":
			timer -= dt
			if timer <= 0.0:
				sim.observe_stock(c)
				sim.add_memory(c, "I checked the stores: %s." % _summary(), 2.0)
				return DONE
	return RUNNING


func _summary() -> String:
	var parts: PackedStringArray = []
	for k in Knowledge.STOCK_KEYS:
		parts.append("%d %s" % [int(sim.stock.get(k, 0)), k])
	return ", ".join(parts)
