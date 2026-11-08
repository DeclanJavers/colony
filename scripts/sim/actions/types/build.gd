extends ActionBase
## Start a new building (decision extra "build_what" = type) or help an unfinished site
## ("continue"). Works up to MAX_SESSION minutes, then the colonist decides again.

const MAX_SESSION := 180.0

var site: Building
var worked: float = 0.0


func kind() -> String:
	return "build"


func available() -> bool:
	return not sim.build_options().is_empty()


func begin() -> bool:
	var what: String = extra.get("build_what", "")
	var opts := sim.build_options()
	if opts.is_empty():
		return false
	if not opts.has(what):
		what = opts[0]
	if what == "continue":
		site = sim.nearest_reachable_site(c.cell)
	else:
		# Find a reachable spot BEFORE paying, so a site can never be orphaned.
		var spot := sim.find_build_spot(what, c.cell)
		if spot.x < 0:
			return false
		site = sim.place_site(what, c, false, spot)
	if site == null:
		return false
	if not go_to(sim.map.access_cell(site.cells(), c.cell)):
		return false
	phase = "walking"
	return true


func describe() -> String:
	if site == null:
		return "build"
	return "build %s (%d%%)" % [site.def().get("label", site.type), int(site.progress() * 100)]


func tick(dt: float) -> int:
	if site.complete:
		return DONE
	match phase:
		"walking":
			if walk(dt):
				phase = "working"
		"working":
			site.work_done += dt * work_rate("building") * sim.colony_mult("build_speed")
			c.add_skill_xp("building", dt)
			worked += dt
			if site.work_done >= site.work_needed:
				sim.complete_building(site, c)
				return DONE
			if worked >= MAX_SESSION:
				return DONE
	return RUNNING
