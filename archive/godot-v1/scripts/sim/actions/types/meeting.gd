extends ActionBase
## Go to the weekly meeting (see meetings.gd) and stay until it's over: inside the hall if there's
## a seat, else standing outside it. Offered only while the meeting is gathering.

var place: Building


func kind() -> String:
	return "socialize"


func available() -> bool:
	return sim.meetings.open_for(c) and sim.meetings.meeting_place() != null


func begin() -> bool:
	place = sim.meetings.meeting_place()
	if place == null or not go_to(sim.map.access_cell(place.cells(), c.cell)):
		return false
	phase = "walking"
	return true


func describe() -> String:
	match phase:
		"inside":
			return "at the meeting (inside)"
		"outside":
			if place != null and place.provides("meeting_place") == null:
				return "at the meeting by the campfire"
			return "at the meeting (standing outside)"
	return "going to the meeting"


func tick(dt: float) -> int:
	match phase:
		"walking":
			if sim.meetings.is_over():
				return fail("the meeting was over")
			if walk(dt):
				phase = "inside" if sim.meetings.arrive(c) else "outside"
		"inside", "outside":
			c.needs["social"] = minf(100.0, float(c.needs.get("social", 100.0)) + 20.0 * dt / 60.0)
			if sim.meetings.is_over():
				return DONE
	return RUNNING
