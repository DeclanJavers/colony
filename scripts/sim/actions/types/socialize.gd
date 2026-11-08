extends ActionBase
## Walk to someone and spend time with them. Decision extra "with_whom" = colonist id.
## params: minutes, restore, affection_gain

const GIVE_UP_AFTER := 120.0
const REPATH_EVERY := 10.0

var other: Colonist
var walked: float = 0.0
var repath_timer: float = 0.0


func kind() -> String:
	return "socialize"


func available() -> bool:
	return not sim.social_candidates(c).is_empty()


func begin() -> bool:
	var want := int(extra.get("with_whom", -1))
	other = sim.colonists.get(want) as Colonist
	if other == null or not other.alive() or other == c:
		var cands := sim.social_candidates(c)
		if cands.is_empty():
			return false
		other = cands[0]
	phase = "walking"
	_repath()
	return true


func _repath() -> void:
	go_to(sim.map.access_cell([other.cell], c.cell))
	repath_timer = REPATH_EVERY


func describe() -> String:
	if other == null:
		return "socialize"
	return "%s with %s" % ["chatting" if phase == "talking" else "looking for", other.name]


func tick(dt: float) -> int:
	if other == null or not other.alive():
		return fail("they are gone")
	match phase:
		"walking":
			walked += dt
			if c.pos.distance_to(other.pos) <= 1.6:
				phase = "talking"
				timer = float(params.get("minutes", 40))
				return RUNNING
			if walked > GIVE_UP_AFTER:
				return fail("could not reach %s" % other.name)
			repath_timer -= dt
			if walk(dt) or repath_timer <= 0.0:
				_repath()
		"talking":
			timer -= dt
			if timer <= 0.0:
				sim.finish_social(c, other, params)
				return DONE
	return RUNNING
