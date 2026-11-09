extends ActionBase
## Make a tool (data/tools.json): take the materials from the stores, work at the campfire (or
## the workshop if there is one), and carry the tool from then on. params: tool

var tool := ""
var spot: Building


func kind() -> String:
	return "work"


func available() -> bool:
	tool = String(params.get("tool", ""))
	var d: Dictionary = sim.content.tools.get(tool, {})
	return not d.is_empty() and not c.has_tool(tool) and sim.affordable(d.get("cost", {}))


func begin() -> bool:
	tool = String(params.get("tool", ""))
	var d: Dictionary = sim.content.tools.get(tool, {})
	if d.is_empty() or c.has_tool(tool) or not sim.affordable(d.get("cost", {})):
		return false
	spot = sim.nearest_building_with("colony", c.cell)   # the workshop, if any
	if spot == null or spot.type != "workshop":
		spot = sim.nearest_building_with("rest_spot", c.cell)
	if spot != null and not go_to(sim.map.access_cell(spot.cells(), c.cell)):
		spot = null
	for r in d.get("cost", {}):
		sim.stock[r] = float(sim.stock[r]) - float(d["cost"][r])
	sim.events.stock_changed.emit(sim.stock)
	phase = "walking" if spot != null else "working"
	timer = float(d.get("minutes", 40))
	return true


func describe() -> String:
	return "making a %s" % sim.content.tools.get(tool, {}).get("label", tool)


func tick(dt: float) -> int:
	match phase:
		"walking":
			if walk(dt):
				phase = "working"
		"working":
			timer -= dt * work_rate("crafting")
			c.add_skill_xp("crafting", dt)
			if timer <= 0.0:
				sim.give_tool(c, tool)
				return DONE
	return RUNNING
