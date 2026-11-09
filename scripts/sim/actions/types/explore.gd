extends ActionBase
## Head out toward the part of the map this colonist knows least about, looking around on the
## way (everything within sight is discovered as they walk). params: distance, minutes

var target := Vector2i(-1, -1)
var found := 0


func kind() -> String:
	return "work"


func begin() -> bool:
	target = sim.exploration_target(c, int(params.get("distance", 14)))
	if target.x < 0 or not go_to(target):
		return false
	phase = "walking"
	return true


func describe() -> String:
	return "exploring" if phase == "walking" else "looking around"


func tick(dt: float) -> int:
	match phase:
		"walking":
			found += sim.look_around(c)
			if walk(dt):
				phase = "looking"
				timer = float(params.get("minutes", 10))
		"looking":
			found += sim.look_around(c)
			timer -= dt
			if timer <= 0.0:
				sim.add_memory(c, "I explored %s and found %s." % [sim.direction_word(c.cell), "%d new places" % found if found > 0 else "nothing new"], 2.5 if found > 0 else 1.0)
				return DONE
	return RUNNING
