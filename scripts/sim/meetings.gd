class_name ColonyMeetings
extends RefCounted
## The weekly meeting (data/meetings.json). On meeting days the bell rings: adults who can come
## walk to the meeting hall (or the campfire, before there is a hall). The first to arrive sit
## inside, up to the hall's seats; latecomers stand outside, and a crowded hall is remembered,
## which is what makes people want to enlarge it. Once people have gathered they talk: everyone
## hears the freshest news anyone there has (stock and places), and they agree a short plan
## (what the colony should do, and who will do it). The plan lasts until the next meeting; people
## who missed it hear about it in conversation.
##
## The talk itself is written by the chat model when the game has one (Sim.meeting_handler);
## otherwise each person says what they believe is most needed and the plan is what most said.

var sim: Sim
## The meeting happening now, or {}: {day, start, end, talk_at, place: Building, at: Vector2i,
## seats, present: [ids], inside: [ids], talked: bool}
var current: Dictionary = {}
## The plan agreed at the last meeting, or {}: {day, t, items: [{action, build_what, label, who}],
## present: [names], seats, overflow, lines: [{who, text}], llm}
var plan: Dictionary = {}
var held: int = 0
var last_overflow: int = 0      # people who had to stand outside at the last meeting


## Building jobs in the plan are done once someone starts them ("build a house" means one house).
func watch(p_sim: Sim) -> void:
	sim = p_sim
	sim.events.building_placed.connect(func(b: Building) -> void:
		_mark_done(b.type)
		if b.provides("meeting_place") != null:
			last_overflow = 0)        # the crush round the campfire is being dealt with
	sim.events.building_expansion_started.connect(func(b: Building) -> void:
		_mark_done("expand:" + b.type)
		last_overflow = 0)            # being enlarged: last week's crowding is answered


func _mark_done(build_what: String) -> void:
	for it in plan.get("items", []):
		if String(it.get("build_what", "")) == build_what and not bool(it.get("done", false)):
			it["done"] = true
			return


## Plan items still to do.
func open_items() -> Array:
	return plan.get("items", []).filter(func(it) -> bool: return not bool(it.get("done", false)))


func cfg(key: String, default: Variant) -> Variant:
	return sim.content.meetings.get(key, default)


func is_meeting_day(day: int) -> bool:
	var every := int(cfg("every_days", 7))
	return day >= int(cfg("first_day", every)) and day % every == 0


## Hours until the next bell (for the state text), or -1 if more than a day away.
func hours_until_next() -> float:
	if not is_meeting_day(sim.day()):
		return -1.0
	var h := float(cfg("hour", 17)) - sim.time_of_day()
	return h if h > 0.0 else -1.0


## Where meetings happen: the meeting hall, or the campfire until there is one.
func meeting_place() -> Building:
	var hall: Building = null
	var fire: Building = null
	for b: Building in sim.buildings.values():
		if b.provides("meeting_place") != null and (b.complete or b.expanding):
			hall = b
		elif b.complete and b.provides("rest_spot") != null and fire == null:
			fire = b
	return hall if hall != null else fire


func on_hour() -> void:
	if current.is_empty() and is_meeting_day(sim.day()) and sim.hour() == int(cfg("hour", 17)):
		_ring_bell()


## Called every sim minute while a meeting is on (talk and end aren't tied to the hour).
func update() -> void:
	if current.is_empty():
		return
	if not bool(current["talked"]) and sim.clock >= float(current["talk_at"]):
		_hold_talk()
	if sim.clock >= float(current["end"]):
		current = {}


func _ring_bell() -> void:
	var place := meeting_place()
	if place == null:
		return
	var length := float(cfg("length_hours", 2.0)) * 60.0
	current = {"day": sim.day(), "start": sim.clock, "end": sim.clock + length,
		"talk_at": sim.clock + float(cfg("arrive_within_hours", 1.0)) * 60.0,
		"place": place, "seats": seats_of(place),
		"present": [], "inside": [], "talked": false}
	sim.events.meeting_called.emit(current)
	# Everyone hears the bell: a request from the meeting itself, which they weigh like any other
	# (someone starving still eats first).
	for c: Colonist in sim.colonists.values():
		if c.is_adult():
			sim.add_request(c, "the meeting bell", "attend_meeting", 1.0, true)


## How many fit: the hall's seats for its size, or how many can sit round the campfire.
func seats_of(place: Building) -> int:
	if place == null:
		return 0
	if place.provides("meeting_place") != null:
		return place.seats()
	return int(place.provides("meeting_seats", 0))


## Is there a meeting `c` can still get to?
func open_for(c: Colonist) -> bool:
	return not current.is_empty() and c.is_adult() and not bool(current["talked"]) \
		and not (current["present"] as Array).has(c.id)


## Take a seat if there's one left (inside the hall, or round the campfire). Returns true only
## when they go inside the hall (out of sight, like at home at night).
func arrive(c: Colonist) -> bool:
	if current.is_empty():
		return false
	var present: Array = current["present"]
	if not present.has(c.id):
		present.append(c.id)
	var inside: Array = current["inside"]
	if int(current["seats"]) > 0 and inside.size() < int(current["seats"]) and not inside.has(c.id):
		inside.append(c.id)
		return (current["place"] as Building).provides("meeting_place") != null
	return false


func is_inside(c: Colonist) -> bool:
	return not current.is_empty() and (current["inside"] as Array).has(c.id)


func is_over() -> bool:
	return current.is_empty() or sim.clock >= float(current["end"])


func _present() -> Array[Colonist]:
	var out: Array[Colonist] = []
	for cid in current["present"]:
		var c := sim.colonists.get(cid) as Colonist
		if c != null and c.alive():
			out.append(c)
	return out


func _hold_talk() -> void:
	current["talked"] = true
	var people := _present()
	if people.size() < 2:
		return
	_pool_knowledge(people)
	var meeting := {"day": current["day"], "t": sim.clock, "people": people, "options": plan_options(people),
		"seats": current["seats"], "overflow": maxi(0, people.size() - int(current["seats"])) if int(current["seats"]) > 0 else 0,
		"place": current["place"], "inside": (current["inside"] as Array).duplicate()}
	if sim.meeting_handler.is_valid() and bool(sim.meeting_handler.call(meeting)):
		return   # the chat model writes it and calls apply() when done
	apply(meeting, symbolic(meeting))


## Everyone at the meeting hears the freshest news anyone there has, and where things are.
func _pool_knowledge(people: Array[Colonist]) -> void:
	for k in Knowledge.STOCK_KEYS:
		var best: Colonist = null
		var best_t := -INF
		for p in people:
			if p.knowledge.knows(k, sim.clock) and float(p.knowledge.facts[k]["t"]) > best_t:
				best_t = float(p.knowledge.facts[k]["t"])
				best = p
		if best == null:
			continue
		for p in people:
			if p != best:
				p.knowledge.hear(k, best.knowledge.tell(k, sim.clock), best_t, best.name, sim.trust(p, best), sim.clock)
	for li in people:
		for sp in people:
			if sp != li:
				sim.share_places(sp, li, 2)
				li.knowledge.roles[sp.id] = sp.role    # everyone sees who does what


## What the meeting could agree to do: [{id, label}] (gather jobs, explore, and building).
func plan_options(people: Array[Colonist]) -> Array:
	var out := []
	for aid in ["gather_food", "fetch_water", "gather_wood", "gather_stone", "farm", "explore"]:
		var d: Dictionary = sim.content.actions_by_id.get(aid, {})
		if not d.is_empty():
			out.append({"id": aid, "label": String(d.get("label", aid))})
	for t in sim.content.buildings:
		var d: Dictionary = sim.content.buildings[t]
		if int(d.get("tier", 0)) <= sim.unlocked_tier and sim.count_of(t) < int(d.get("max_count", 999)):
			out.append({"id": "build:" + t, "label": "build a %s" % d.get("label", t)})
	for b: Building in sim.buildings.values():
		if b.complete and not b.expanding and not b.level_def(b.level + 1).is_empty():
			out.append({"id": "expand:" + b.type, "label": "make the %s bigger" % b.def().get("label", b.type)})
	return out


## Without the chat model: each says what they think is most needed (from what they believe),
## the plan is what most people said, and the people best at each job volunteer.
func symbolic(meeting: Dictionary) -> Dictionary:
	var people: Array = meeting["people"]
	var ids := {}
	for o in meeting["options"]:
		ids[o["id"]] = o["label"]
	var votes := {}
	var lines := []
	var said := {}   # wish -> who said it first
	for p: Colonist in people:
		var wants := _wishes(p, meeting, ids).slice(0, 2)
		for k in wants.size():
			votes[wants[k]] = float(votes.get(wants[k], 0.0)) + (1.0 if k == 0 else 0.6)
		if wants.is_empty() or lines.size() >= 6:
			continue
		var fresh: Array = wants.filter(func(x) -> bool: return not said.has(x))
		if not fresh.is_empty():
			said[fresh[0]] = p.name
			lines.append({"who": p.name, "text": _say_wish(fresh[0], ids, lines.size())})
		else:
			lines.append({"who": p.name, "text": "I agree with %s." % said[wants[0]]})
	if lines.is_empty():
		var chair: Colonist = people[0]
		for p: Colonist in people:
			if p.age_minutes > chair.age_minutes:
				chair = p
		lines.append({"who": chair.name, "text": "Things are going well. Let's keep on as we are."})
	var ranked: Array = votes.keys()
	ranked.sort_custom(func(a, b) -> bool: return float(votes[a]) > float(votes[b]))
	var items := []
	for id in ranked.slice(0, int(cfg("plan_items", 3))):
		items.append({"id": id})
	return {"lines": lines, "items": _assign(items, people), "llm": false}


## What `p` would raise at the meeting, most pressing first (from what they believe and saw).
func _wishes(p: Colonist, meeting: Dictionary, ids: Dictionary) -> Array:
	var out := []
	var add := func(id: String) -> void:
		if ids.has(id) and not out.has(id):
			out.append(id)
	# Standing outside a full hall is what makes people want a bigger one.
	if int(meeting["overflow"]) > 0 and not (meeting.get("inside", []) as Array).has(p.id) and place_is_hall(meeting):
		add.call("expand:meeting_hall")
	var short := sim.believed_shortage(p)
	if short != "":
		add.call(String(Sim.GATHER_FOR.get(short, "")))
	# Their own reasons for a house: no bed, or a child on the way and nowhere for it.
	if p.home_id < 0 or (p.pregnant_until > 0.0 and sim.free_beds() <= 0):
		add.call("build:house")
	# Crowded round the campfire: a hall would fit everyone.
	var place: Building = meeting["place"]
	if place != null and place.provides("meeting_place") == null and int(meeting["overflow"]) > 0:
		add.call("build:meeting_hall")
	if sim.season() != "winter" and sim.days_until("winter") <= int(sim.content.seasons.get("days_per_season", 4)):
		add.call("gather_wood")
		add.call("farm")
	for res in ["stone", "wood", "food"]:
		if p.knowledge.usable_places_of(_node_for(res), p.cell, sim.clock, res == "food").is_empty():
			add.call("explore")
	return out


func place_is_hall(meeting: Dictionary) -> bool:
	return meeting["place"] != null and (meeting["place"] as Building).provides("meeting_place") != null


func _node_for(res: String) -> String:
	for t in sim.content.nodes:
		if String(sim.content.nodes[t].get("resource", "")) == res:
			return t
	return ""


func people_count(meeting: Dictionary) -> int:
	return (meeting["people"] as Array).size()


func _say_wish(id: String, ids: Dictionary, k: int = 0) -> String:
	if id.begins_with("expand:"):
		return ["We barely fit in here. We should %s.", "Half of us are standing outside. Let's %s."][k % 2] % ids[id]
	if id.begins_with("build:"):
		return ["I think we should %s.", "What we need is to %s."][k % 2] % ids[id]
	if id == "explore":
		return ["Someone should go and explore; we don't know where to find enough.", "We need to look further out for what we're missing."][k % 2]
	return ["We're running short. Someone should %s.", "We should %s, we're low.", "I'd like a few of us to %s."][k % 3] % ids[id]


## Who does what: for each item, people known for that work first, then whoever is best at it;
## nobody gets two jobs if it can be helped.
func _assign(items: Array, people: Array) -> Array:
	var taken := {}
	for it in items:
		_fill_action(it)
		var sk := sim.skill_for_action(String(it["action"]))
		var ranked: Array = people.filter(func(p: Colonist) -> bool: return not taken.has(p.id))
		ranked.sort_custom(func(a: Colonist, b: Colonist) -> bool:
			var ra := 1 if a.role == sk else 0
			var rb := 1 if b.role == sk else 0
			if ra != rb:
				return ra > rb
			return float(a.skills.get(sk, 0.0)) > float(b.skills.get(sk, 0.0)))
		var who: Array = []
		var n := clampi(roundi(people.size() * float(cfg("volunteer_share", 0.25))), 1, int(cfg("max_volunteers", 3)))
		for p: Colonist in ranked.slice(0, n):
			who.append(p.name)
			taken[p.id] = true
		it["who"] = who
	return items


## Plan item id -> the action that carries it out ("build:house" -> build, build_what house).
func _fill_action(it: Dictionary) -> void:
	var id: String = it["id"]
	it["action"] = id
	it["build_what"] = ""
	if id.begins_with("build:"):
		it["action"] = "build"
		it["build_what"] = id.substr(6)
	elif id.begins_with("expand:"):
		it["action"] = "build"
		it["build_what"] = id


## Apply what the meeting agreed: the plan, memories, requests for the volunteers, and how
## crowded it was. `result` = {lines, items: [{id, who?}], llm}.
func apply(meeting: Dictionary, result: Dictionary) -> void:
	var people: Array = meeting["people"]
	var by_name := {}
	for p: Colonist in people:
		by_name[p.name] = p
	var ids := {}
	for o in meeting["options"]:
		ids[o["id"]] = o["label"]
	var items := []
	for it in result.get("items", []):
		var id := String(it.get("id", ""))
		if not ids.has(id) or items.any(func(x) -> bool: return x["id"] == id):
			continue
		# Volunteers named by the chat model: real attendees, once each, one job per person.
		var who: Array = []
		for n in it.get("who", []):
			if by_name.has(n) and not who.has(n) and not items.any(func(x) -> bool: return (x["who"] as Array).has(n)) \
					and who.size() < int(cfg("max_volunteers", 3)):
				who.append(n)
		items.append({"id": id, "who": who})
		if items.size() >= int(cfg("plan_items", 3)):
			break
	# Items the chat model left unassigned get volunteers the usual way.
	var unassigned := items.filter(func(x) -> bool: return (x["who"] as Array).is_empty())
	_assign(unassigned, people.filter(func(p: Colonist) -> bool:
		return not items.any(func(x) -> bool: return (x["who"] as Array).has(p.name))))
	for it in items:
		_fill_action(it)
		it["label"] = ids[it["id"]]
	held += 1
	last_overflow = int(meeting["overflow"])
	plan = {"day": meeting["day"], "t": sim.clock, "items": items, "present": people.map(func(p: Colonist) -> String: return p.name),
		"seats": meeting["seats"], "overflow": last_overflow, "lines": result.get("lines", []), "llm": result.get("llm", false),
		"place": (meeting["place"] as Building).def().get("label", "campfire") if meeting["place"] != null else ""}
	var summary := plan_summary()
	for p: Colonist in people:
		p.plan_day = int(plan["day"])
		sim.add_memory(p, "At the meeting we agreed: %s." % summary, 3.5)
		for it in items:
			if (it["who"] as Array).has(p.name):
				sim.add_request(p, "the meeting", String(it["action"]), 0.9)
		if last_overflow > 0 and not (meeting.get("inside", []) as Array).has(p.id):
			sim.add_memory(p, "The %s was too full at the meeting; I had to stand outside." % plan["place"], 2.5)
	sim.events.meeting_held.emit(plan)


## "chop wood (Zed, Pia), build a house (Theo)"
func plan_summary(p: Dictionary = plan) -> String:
	var parts: PackedStringArray = []
	for it in p.get("items", []):
		var who: Array = it.get("who", [])
		parts.append("%s%s%s" % [it["label"], " (%s)" % ", ".join(who) if not who.is_empty() else "",
			" (done)" if bool(it.get("done", false)) else ""])
	return ", ".join(parts) if not parts.is_empty() else "nothing in particular"


## Does `c` know the current plan (was there, or heard about it), and is it still current?
func knows_plan(c: Colonist) -> bool:
	return not plan.is_empty() and c.plan_day == int(plan["day"])


## Someone who knows the plan tells someone who doesn't (called from conversations).
func pass_on(from: Colonist, to: Colonist) -> bool:
	if not knows_plan(from) or knows_plan(to):
		return false
	to.plan_day = int(plan["day"])
	sim.add_memory(to, "%s told me what the meeting decided: %s." % [from.name, plan_summary()], 2.5)
	return true


func to_dict() -> Dictionary:
	var cur := {}
	if not current.is_empty():
		cur = current.duplicate(true)
		cur["place"] = (current["place"] as Building).id if current["place"] != null else -1
	return {"plan": plan.duplicate(true), "held": held, "last_overflow": last_overflow, "current": cur}


## Call after the buildings are loaded (a meeting in progress refers to its place).
func from_dict(d: Dictionary) -> void:
	plan = d.get("plan", {}).duplicate(true)
	held = int(d.get("held", 0))
	last_overflow = int(d.get("last_overflow", 0))
	current = d.get("current", {}).duplicate(true)
	if not current.is_empty():
		current["place"] = sim.buildings.get(int(current["place"]))
		if current["place"] == null:
			current = {}
