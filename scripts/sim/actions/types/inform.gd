extends ActionBase
## Go to someone on purpose to tell them something. params: intent ("news" | "ask"), minutes.
## Decision extra "with_whom" = who to go to. The topic comes from what the colonist believes:
##   news: share what they know (they must have seen something in the last few hours),
##   ask:  ask for help with what they believe the colony is shortest of.

const FRESH_H := 6.0
const GIVE_UP_AFTER := 120.0

var other: Colonist
var topic := ""
var walked := 0.0
var repath := 0.0


func kind() -> String:
	return "socialize"


func available() -> bool:
	if sim.social_candidates(c).is_empty():
		return false
	if params.get("intent", "news") == "ask":
		return sim.believed_shortage(c) != ""
	for k in Knowledge.STOCK_KEYS:
		var f: Dictionary = c.knowledge.facts.get(k, {})
		if not f.is_empty() and f["src"] == "seen" and c.knowledge.age_h(k, sim.clock) < FRESH_H:
			return true
	return false


func begin() -> bool:
	other = sim.colonists.get(int(extra.get("with_whom", -1))) as Colonist
	if other == null or other == c or not other.alive():
		var cands := sim.social_candidates(c)
		if cands.is_empty():
			return false
		other = cands[0]
	if params.get("intent", "news") == "ask":
		topic = String(Sim.GATHER_FOR.get(sim.believed_shortage(c), ""))
		if topic == "":
			return false
	phase = "walking"
	go_to(sim.map.access_cell([other.cell], c.cell))
	repath = 10.0
	return true


func describe() -> String:
	if other == null:
		return String(def.get("label", "talk"))
	var what := "asking %s for help" % other.name if params.get("intent") == "ask" else "telling %s the news" % other.name
	return what if phase == "talking" else "going to find %s" % other.name


func tick(dt: float) -> int:
	if other == null or not other.alive():
		return fail("they are gone")
	match phase:
		"walking":
			walked += dt
			if c.pos.distance_to(other.pos) <= 1.6:
				phase = "talking"
				timer = float(params.get("minutes", 10))
				sim.converse(c, other, String(params.get("intent", "news")), topic)
				return RUNNING
			if walked > GIVE_UP_AFTER:
				return fail("could not reach %s" % other.name)
			repath -= dt
			if walk(dt) or repath <= 0.0:
				go_to(sim.map.access_cell([other.cell], c.cell))
				repath = 10.0
		"talking":
			timer -= dt
			if timer <= 0.0:
				return DONE
	return RUNNING
