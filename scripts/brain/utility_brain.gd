class_name UtilityBrain
extends Brain
## Hand-tuned baseline brain. Runs instantly with no Python, so the sim can be tested
## headless and Laya's choices can be compared against something sensible.

const RESOURCE_TARGETS := {"food": 80.0, "water": 40.0, "wood": 60.0, "stone": 50.0}
const KIND_OF := {
	"gather_wood": "gather", "gather_stone": "gather", "gather_food": "gather", "fetch_water": "gather",
	"farm": "work", "build": "build", "socialize": "socialize", "have_child": "socialize",
	"rest": "rest", "sleep": "rest", "check_stores": "work", "share_news": "socialize", "ask_help": "socialize", "warm_up": "rest", "explore": "work", "attend_meeting": "socialize", "make_hatchet": "work", "make_pickaxe": "work", "make_rod": "work", "fish": "gather",
}

var temperature: float = 0.12
var pick: String = "argmax"


func _init(p_pick: String = "argmax") -> void:
	id = "utility"
	pick = p_pick


func decide(ctx: Dictionary, sim: Sim, done: Callable) -> void:
	var t0 := Time.get_ticks_usec()
	var c := sim.colonists.get(int(ctx["colonist_id"])) as Colonist
	var scores := {}
	for o in ctx["options"]:
		scores[o["id"]] = _score(o["id"], c, sim)
	# Building only makes sense if something is worth building: nothing scoring above zero means
	# whatever got built would be waste (e.g. the only affordable thing is another farm).
	if scores.has("build") and _best_build(sim) <= 0.0:
		scores["build"] = minf(float(scores["build"]), -1.0)
	var probs := Brain.softmax(scores, temperature)
	var action := Brain.argmax(probs) if pick == "argmax" else Brain.sample(probs, sim.rng)

	var bscores := {}
	for o in ctx["build_options"]:
		bscores[o["id"]] = _build_score(o["id"], sim)
	var bprobs := Brain.softmax(bscores, 0.15) if not bscores.is_empty() else {}

	var sscores := {}
	for o in ctx["social_options"]:
		var oid := int(o["id"])
		var aff := float(c.affection.get(oid, 0.0))
		sscores[o["id"]] = aff / 50.0 + (1.0 if c.partner_id == oid else 0.0)
	var sprobs := Brain.softmax(sscores, 0.3) if not sscores.is_empty() else {}
	# Asking for help: go to someone they believe is known for that work, if there's one nearby
	# enough, rather than just whoever they like best.
	if action == "ask_help":
		var topic := String(Sim.GATHER_FOR.get(sim.believed_shortage(c), ""))
		var sk := sim.skill_for_action(topic)
		var reach := float(sim.content.roles.get("ask_reach_tiles", 30))
		var best_d := INF
		for oid in c.knowledge.roles:
			var o := sim.colonists.get(oid) as Colonist
			if o != null and o != c and sk != "" and String(c.knowledge.roles[oid]) == sk and o.pos.distance_to(c.pos) < minf(reach, best_d):
				best_d = o.pos.distance_to(c.pos)
				sprobs = {str(o.id): 1.0}

	done.call({
		"action": action,
		"build_what": Brain.argmax(bprobs) if not bprobs.is_empty() else "",
		"with_whom": int(Brain.argmax(sprobs)) if not sprobs.is_empty() else -1,
		"probs": probs, "build_probs": bprobs, "social_probs": sprobs,
		"brain": id, "latency_ms": (Time.get_ticks_usec() - t0) / 1000.0,
	})


func _u(c: Colonist, need: String) -> float:
	return pow(1.0 - float(c.needs.get(need, 100.0)) / 100.0, 2.0)


## How short the colony is of `res` *as this colonist believes it*. Unknown stock counts as only
## mildly short: not knowing is a reason to go and look, not to over-gather blindly.
func _scarcity(sim: Sim, c: Colonist, res: String) -> float:
	# Target = a base buffer + what the priciest still-buildable unlocked building needs.
	var target := float(RESOURCE_TARGETS.get(res, 50.0))
	var extra := 0.0
	for t in sim.content.buildings:
		var d: Dictionary = sim.content.buildings[t]
		if int(d.get("tier", 0)) <= sim.unlocked_tier and sim.count_of(t) < int(d.get("max_count", 999)) and _build_score(t, sim) > 0.3:
			extra = maxf(extra, float(d.get("cost", {}).get(res, 0.0)))
	target += _seasonal_reserve(sim, res)
	return clampf(1.0 - _believed(sim, c, res, target + extra) / (target + extra), 0.0, 1.0)


## How far past "enough" the colony already is, as they believe it: 0 up to 1.5x the target, rising
## to 1 at 3x. Hauling more of something the stores are overflowing with is wasted work (and it
## fills storage that other goods need), so it's the teacher's judgement, not a limit.
func _plenty(sim: Sim, c: Colonist, res: String) -> float:
	var target := float(RESOURCE_TARGETS.get(res, 50.0)) + _seasonal_reserve(sim, res)
	return clampf((_believed(sim, c, res, target) / target - 1.5) / 1.5, 0.0, 1.0)


## Does `c` know somewhere to get `res` right now: a resource node they've seen or heard of that
## still has something on it, or for food, farms in a season when crops grow? (Picked-clean
## bushes and winter fields don't count: that's when it's worth looking further afield.)
## Only from what they remember (Knowledge.places and the amounts they last saw there).
func _knows_source(sim: Sim, c: Colonist, res: String) -> bool:
	if res == "food" and sim.count_of("farm_plot", false) > 0 and float(sim.season_def().get("crop_growth", 1.0)) > 0.0:
		return true
	if res == "food" and c.has_tool("rod"):
		return true     # the river
	for t in sim.content.nodes:
		var d: Dictionary = sim.content.nodes[t]
		var need := String(d.get("needs_tool", ""))
		if need != "" and not c.has_tool(need):
			continue      # a tree is no source of wood without a hatchet
		if String(d.get("resource", "")) == res and not c.knowledge.usable_places_of(t, c.cell, sim.clock, float(d.get("regrow_per_hour", 0)) > 0.0).is_empty():
			return true
	return false


func _believed(sim: Sim, c: Colonist, res: String, target: float) -> float:
	return c.knowledge.recall(res, sim.clock) if c.knowledge.knows(res, sim.clock) else target * 0.8


## Stock to put by for the cold. Food for the winter days ahead (nothing grows or regrows then).
## Firewood only needs a couple of days' buffer for every lived-in house: trees can still be
## felled in winter, and storage space is better spent on food that can't be found then.
func _seasonal_reserve(sim: Sim, res: String) -> float:
	if res == "food":
		return _winter_food(sim)
	if res != "wood" or not (sim.season() in ["autumn", "winter"] or sim.days_until("autumn") <= 1):
		return 0.0
	var days := 2.0
	var lived_in := 0
	for b: Building in sim.buildings.values():
		if b.complete and b.provides("heated_warmth") != null and b.beds.any(func(x) -> bool: return int(x) >= 0):
			lived_in += 1
	return lived_in * float(sim.content.seasons.get("firewood_per_house_hour", 0.4)) * 24.0 * days + 20.0


## Food to put by for the lean season: mouths x daily use x the winter days still ahead (counted
## from autumn on). Daily use comes from the data (how fast hunger drains, what a meal restores).
func _winter_food(sim: Sim) -> float:
	var per := int(sim.content.seasons.get("days_per_season", 4))
	var days := 0.0
	if sim.season() == "winter":
		days = per - sim.season_day()
	elif sim.days_until("winter") <= per:
		days = per
	if days <= 0.0:
		return 0.0
	return sim.alive_count() * _daily_food(sim) * days


static var _daily_food_cache := -1.0


static func _daily_food(sim: Sim) -> float:
	if _daily_food_cache < 0.0:
		_daily_food_cache = 6.0
		var decay := float(sim.content.needs.get("hunger", {}).get("decay_per_hour", 4.0)) * 24.0
		for a in sim.content.actions:
			var p: Dictionary = (a as Dictionary).get("params", {})
			if p.get("need", "") == "hunger" and p.get("resource", "") == "food":
				_daily_food_cache = decay / maxf(float(p.get("restore", 50)), 1.0) * float(p.get("use", 1))
				break
	return _daily_food_cache


## Whether working the farm now would only mean planting a field that can't grow (deep winter).
func _farm_planting_useless(sim: Sim, c: Colonist) -> bool:
	if sim.crop_mult() > 0.0:
		return false
	var f := sim.find_farm_job(c.cell)
	return f != null and f.farm_stage == "empty"


func _best_build(sim: Sim) -> float:
	var best := -1.0
	for t in sim.build_options():
		best = maxf(best, _build_score(t, sim))
	return best


func _score(aid: String, c: Colonist, sim: Sim) -> float:
	var s := 0.0
	match aid:
		"drink":
			s = 3.0 * _u(c, "thirst") + (0.3 if float(c.needs["thirst"]) < 60.0 else -0.5)
		"eat":
			s = 3.0 * _u(c, "hunger") + (0.3 if float(c.needs["hunger"]) < 60.0 else -0.5)
		"sleep":
			s = 3.0 * _u(c, "energy")
			if sim.is_night() and float(c.needs["energy"]) < 75.0:
				s += 0.8
			if float(c.needs["energy"]) > 85.0:
				s -= 1.0
		"gather_wood":
			s = 0.35 + 0.8 * _scarcity(sim, c, "wood") - 0.6 * _plenty(sim, c, "wood")
		"gather_stone":
			s = 0.3 + 0.8 * _scarcity(sim, c, "stone") - 0.6 * _plenty(sim, c, "stone")
		"gather_food":
			s = 0.35 + 0.9 * _scarcity(sim, c, "food") - 0.6 * _plenty(sim, c, "food")
		"fetch_water":
			s = 0.3 + 0.8 * _scarcity(sim, c, "water") - 0.6 * _plenty(sim, c, "water")
		"farm":
			# Farming is the big food source; worth more the shorter food is (incl. the winter
			# reserve), and pointless when the job would be planting a field that can't grow.
			s = 0.85 + 0.5 * _scarcity(sim, c, "food")
			if _farm_planting_useless(sim, c):
				s = 0.1
		"build":
			var best := _best_build(sim)
			# Nothing worth building: building anyway (e.g. a house when beds stand empty) is waste.
			s = -0.5 if best < 0.2 else 0.3 + 0.5 * minf(best, 1.5)
		"socialize":
			s = 2.0 * _u(c, "social")
		"have_child":
			# A child is a mouth for every winter to come: worth it when the stores (as they believe
			# them) could carry everyone, the newborn included, through a whole winter.
			var winter_need := float(RESOURCE_TARGETS["food"]) + (sim.alive_count() + 1) * _daily_food(sim) * float(sim.content.seasons.get("days_per_season", 4))
			var have := _believed(sim, c, "food", winter_need)
			s = 1.1 - 1.6 * clampf(1.0 - have / winter_need, 0.0, 1.0)
		"rest":
			s = 0.1 + 0.5 * _u(c, "energy")
		"share_news":
			# Worth telling others when they've freshly seen the colony running short of something.
			var short := sim.believed_shortage(c)
			s = 0.75 if short != "" and c.knowledge.facts[short]["src"] == "seen" else 0.05
			s -= _recently(c, sim, ["share_news", "ask_help"], 8.0)
		"ask_help":
			# Children and worn-out adults ask others; healthy adults mostly just do it themselves.
			var short2 := sim.believed_shortage(c)
			if short2 == "":
				s = 0.0
			elif not c.is_adult():
				s = 1.0
			else:
				s = 0.75 if float(c.needs["energy"]) < 40.0 else 0.15
			s -= _recently(c, sim, ["share_news", "ask_help"], 8.0)
		"warm_up":
			s = 3.0 * _u(c, "warmth") + (0.3 if float(c.needs.get("warmth", 100)) < 50.0 else -0.6)
		"explore":
			# Worth it when nobody they know of has seen a source of something the colony needs
			# (they can't gather what they can't find), and a little while much of the land is unknown.
			var need := 0.0
			for res in ["wood", "stone", "food"]:
				if not _knows_source(sim, c, res):
					need = maxf(need, 0.4 + _scarcity(sim, c, res))
			# Night and bad weather make it a poor time to head into the unknown.
			var seen_share := float(c.knowledge.seen.count(1)) / maxf(1.0, float(c.knowledge.seen.size()))
			s = 1.3 * need + 0.4 * (1.0 - seen_share) - 0.6 - _recently(c, sim, ["explore"], 10.0) \
				- (0.8 if sim.is_night() else 0.0) - 1.5 * (1.0 - sim.weather_work_mult())
		"make_hatchet", "make_pickaxe":
			# Trees and big rocks need the tool. Worth making when you'll need what it gets (the
			# colony is short of it) or when the bare-hands sources you know of are thin.
			var res := "wood" if aid == "make_hatchet" else "stone"
			var no_source := not _knows_source(sim, c, res)
			s = 0.3 + 0.9 * _scarcity(sim, c, res) + (0.5 if no_source else 0.0) - _recently(c, sim, [aid], 6.0)
		"make_rod":
			# A rod is worth making when food is short and they've seen the river to fish in.
			s = (0.2 + 0.9 * _scarcity(sim, c, "food") if sim.seen_shore(c).x >= 0 else -1.0) - _recently(c, sim, [aid], 12.0)
		"fish":
			# Like picking berries: more worth it the shorter food is, and the better fish bite.
			# (They made the rod for this, so a little more than berries when food isn't short.)
			s = (0.42 + 1.0 * _scarcity(sim, c, "food")) * float(sim.season_def().get("fish", 1.0)) - 0.6 * _plenty(sim, c, "food")
		"attend_meeting":
			# The weekly meeting is where news and plans are shared: worth going to unless something
			# urgent comes first (needs score up to ~3). The bell's request adds to this below.
			s = 1.0
		"check_stores":
			# Worth a look when what they know is old or missing.
			s = 1.3 * c.knowledge.uncertainty(sim.clock) - 0.35
	# Requests: doing what someone asked, weighted by how much they trust them and how recent it
	# is. Urgent needs still win (they score up to ~3), so a parched colonist drinks first.
	for r in sim.active_requests(c):
		if r["action"] == aid:
			var fresh := 1.0 - clampf((sim.clock - float(r["t"])) / Sim.REQUEST_TTL, 0.0, 1.0)
			s += 0.9 * float(r["trust"]) * fresh
	# The plan agreed at the meeting (if they know it): its jobs are worth more, and more again for
	# the people who said they'd do them.
	if sim.meetings.knows_plan(c):
		for it in sim.meetings.open_items():
			# A planned building only counts if it can be built now (else it would push building
			# in general, and builders would put up whatever is affordable instead).
			if aid == "build" and not sim.build_options().has(String(it.get("build_what", ""))):
				continue
			if String(it.get("action", "")) == aid:
				s += 0.35 + (0.25 if (it.get("who", []) as Array).has(c.name) else 0.0)
				break
	# Doing what you're good at: people lean towards work they've practised (relative to their own
	# other skills), a little more if it's what they're known for. Practice makes them better at
	# it, so a division of labour can grow by itself; colony needs above still outweigh it.
	var sk := sim.skill_for_action(aid)
	if sk != "":
		var lvl := float(c.skills.get(sk, 0.0))
		var tot := 0.0
		for k in sim.role_skills():
			tot += float(c.skills.get(k, 0.0))
		var avg := tot / maxf(1.0, float(sim.role_skills().size()))
		# Only among work worth doing: being a good builder is no reason to build what nobody needs.
		var lean := float(sim.content.roles.get("lean_per_level", 0.12)) * clampf(lvl - avg, -2.0, 3.0) \
			+ (float(sim.content.roles.get("role_bonus", 0.15)) if c.role == sk else 0.0)
		s += lean * clampf(s, 0.0, 1.0)
	# Trait biases (data/traits.json "utility")
	var kind: String = KIND_OF.get(aid, "need")
	for t in c.traits:
		var bias: Dictionary = sim.content.traits.get(t, {}).get("utility", {})
		s += float(bias.get(kind, 0.0))
		if kind in ["gather", "build"]:
			s += float(bias.get("work", 0.0))
	return s


## 0.7 if they did one of `ids` in the last `hours` (so they don't nag), else 0.
func _recently(c: Colonist, sim: Sim, ids: Array, hours: float) -> float:
	for r in c.recent:
		if ids.has(r["action"]) and sim.clock - float(r["t"]) < hours * 60.0:
			return 0.7
	return 0.0


func _build_score(bid: String, sim: Sim) -> float:
	if bid == "continue":
		return 1.2
	var planned := 0.0
	for it in sim.meetings.open_items():
		if String(it.get("build_what", "")) == bid:
			planned = 0.6     # the colony agreed to build it
	if bid.begins_with("expand:"):
		# Enlarge the hall when the last meeting didn't fit everyone, or the adults no longer would.
		var hall: Building = null
		for b: Building in sim.buildings.values():
			if b.type == bid.substr(7):
				hall = b
		# Worth it when the last meeting didn't fit everyone (people remember standing outside).
		var crowded := sim.meetings.last_overflow > 0 and hall != null and not hall.expanding
		return (0.9 if crowded else -0.8) + planned
	var s := 0.6 - 0.05 * float(sim.content.buildings[bid].get("tier", 0))
	match bid:
		"campfire":
			s += 1.0
		"well":
			s += 0.8 if sim.count_of("well") == 0 else -0.3
		"farm_plot":
			# Enough fields for the people there are: another one isn't needed (and costs wood
			# that houses need). The teacher's judgement, not a game rule.
			# While food is short of what the colony will need (winter included), more fields pay:
			# about one per three people instead of one per four. No point starting one in winter.
			var food_goal := float(RESOURCE_TARGETS["food"]) + _winter_food(sim)
			var short := float(sim.stock.get("food", 0.0)) < food_goal
			var enough := 1 + sim.alive_count() / (3 if short else 4)
			s += 0.7 if sim.count_of("farm_plot") < enough else -1.0
			# A field is worth as much as crops grow this season (data/seasons.json): nothing in winter.
			s -= 1.0 - clampf(float(sim.season_def().get("crop_growth", 1.0)), 0.0, 1.0)
		"house":
			s += 0.9 if sim.free_beds() <= 1 else -0.6
		"stockpile", "storehouse":
			s += 0.9 if sim.stock_total() > sim.storage_capacity() * 0.8 else -0.4
		"workshop", "kitchen", "town_hall":
			s += 0.4
		"meeting_hall":
			# Somewhere to meet once the gatherings round the campfire no longer fit everyone.
			s += 0.5 if sim.meetings.last_overflow > 0 else -0.3
	s += planned
	return s
