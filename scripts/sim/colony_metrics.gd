class_name ColonyMetrics
extends RefCounted
## Watches a colony and measures how well it's being run, for comparing brains and models.
## Nothing here changes behaviour: it only observes. Flags are thresholds for "this looks
## unhealthy" so failures (farm spam, homelessness, famine, gluts...) show up as numbers.

var sim: Sim
var hours := 0
var homeless_hours := 0.0          # colonist-hours without a bed
var person_hours := 0.0
var critical_hours := 0.0          # colonist-hours with thirst/hunger < 15
var cold_hours := 0.0              # colonist-hours with warmth < 30
var winter_wood := -1.0            # wood in stock when the first winter starts
var famine_hours := 0              # hours with less than a day of food in storage
var max_glut := {}                 # resource -> max days of supply in storage
var max_farms_per_person := 0.0
var _attend := 0.0       # sum of attendance shares (present / adults) over meetings
var _attend_n := 0.0
var belief_err_sum := 0.0
var belief_err_n := 0
var talks := {}
var tier4_day := -1
var choices := {}

var daily_use := {}   # resource -> per person per day, worked out from needs.json + actions.json


func attach(p_sim: Sim) -> void:
	sim = p_sim
	# Per-person daily use = (need decay per day / restore per meal) x amount per meal.
	for a in sim.content.actions:
		if a.get("type") == "consume":
			var p: Dictionary = a["params"]
			var decay := float(sim.content.needs.get(p["need"], {}).get("decay_per_hour", 4.0)) * 24.0
			daily_use[p["resource"]] = decay / maxf(float(p.get("restore", 50)), 1.0) * float(p.get("use", 1))
	sim.events.hour_passed.connect(_on_hour)
	sim.events.conversation_started.connect(func(conv: Dictionary) -> void:
		talks[conv["intent"]] = int(talks.get(conv["intent"], 0)) + 1)
	sim.events.meeting_held.connect(func(p: Dictionary) -> void:
		var adults := 0
		for c: Colonist in sim.colonists.values():
			if c.is_adult():
				adults += 1
		_attend += float((p["present"] as Array).size()) / maxf(1.0, float(adults))
		_attend_n += 1.0)
	sim.events.tier_unlocked.connect(func(t: int) -> void:
		if t >= 4 and tier4_day < 0:
			tier4_day = sim.day())
	sim.events.decision_made.connect(func(_c: Colonist, _ctx: Dictionary, d: Dictionary, _b: bool) -> void:
		choices[d.get("action", "")] = int(choices.get(d.get("action", ""), 0)) + 1)


func _on_hour(_d: int, _h: int) -> void:
	hours += 1
	var pop := sim.alive_count()
	if pop == 0:
		return
	for c: Colonist in sim.colonists.values():
		person_hours += 1.0
		if c.home_id < 0:
			homeless_hours += 1.0
		if float(c.needs.get("thirst", 100)) < 15.0 or float(c.needs.get("hunger", 100)) < 15.0:
			critical_hours += 1.0
		if float(c.needs.get("warmth", 100)) < 30.0:
			cold_hours += 1.0
		for k in Knowledge.STOCK_KEYS:
			if c.knowledge != null and c.knowledge.knows(k, sim.clock):
				var truth := float(sim.stock.get(k, 0.0))
				belief_err_sum += absf(c.knowledge.recall(k, sim.clock) - truth) / maxf(truth, 20.0)
				belief_err_n += 1
	if winter_wood < 0.0 and sim.season() == "winter":
		winter_wood = float(sim.stock.get("wood", 0.0))
	if float(sim.stock.get("food", 0.0)) < pop * float(daily_use.get("food", 6.0)):
		famine_hours += 1
	for k in daily_use:
		max_glut[k] = maxf(float(max_glut.get(k, 0.0)), float(sim.stock.get(k, 0.0)) / (pop * float(daily_use[k])))
	for k in ["wood", "stone"]:   # building materials: measured against the dearest building
		max_glut[k] = maxf(float(max_glut.get(k, 0.0)), float(sim.stock.get(k, 0.0)) / 200.0)
	max_farms_per_person = maxf(max_farms_per_person, float(sim.count_of("farm_plot", false)) / pop)


func report() -> Dictionary:
	var deaths: Dictionary = sim.stats.get("deaths", {})
	var decisions := int(sim.stats.get("decisions", 0))
	var r := {
		"pop": sim.alive_count(), "births": int(sim.stats.get("births", 0)), "deaths": deaths,
		"tier": sim.unlocked_tier, "tier4_day": tier4_day,
		"houses": sim.count_of("house", false), "farms": sim.count_of("farm_plot", false),
		"homeless_share": snappedf(homeless_hours / maxf(person_hours, 1.0), 0.01),
		"critical_share": snappedf(critical_hours / maxf(person_hours, 1.0), 0.001),
		"cold_share": snappedf(cold_hours / maxf(person_hours, 1.0), 0.001), "winter_wood": winter_wood,
		"explored_share": snappedf(sim.explored_share(), 0.01), "roles": _roles(), "meetings": sim.meetings.held, "meeting_attendance": snappedf(_attend / maxf(1.0, _attend_n), 0.01), "hall_seats": _hall_seats(), "last_plan": sim.meetings.plan_summary(),
		"famine_share": snappedf(float(famine_hours) / maxf(hours, 1.0), 0.01),
		"max_farms_per_person": snappedf(max_farms_per_person, 0.01),
		"max_glut_days": {}, "failed_share": snappedf(float(sim.stats.get("failed_decisions", 0)) / maxf(decisions, 1.0), 0.001),
		"decisions": decisions, "talks": talks,
		"requests_made": int(sim.stats.get("requests_made", 0)), "requests_followed": int(sim.stats.get("requests_followed", 0)),
		"belief_error": snappedf(belief_err_sum / maxf(belief_err_n, 1.0), 0.01), "choices": choices,
	}
	for k in max_glut:
		r["max_glut_days"][k] = snappedf(float(max_glut[k]), 0.1)
	r["flags"] = flags(r)
	return r


## What looks unhealthy. Thresholds are judgement calls, documented here, not game rules.
static func flags(r: Dictionary) -> Array:
	var f := []
	var d: Dictionary = r["deaths"]
	if int(d.get("starvation", 0)) + int(d.get("thirst", 0)) > 0:
		f.append("starvation/thirst deaths")
	if int(d.get("freezing", 0)) > 0:
		f.append("people froze to death")
	if float(r.get("cold_share", 0.0)) > 0.1:
		f.append("people often cold (%d%% of the time)" % int(100 * float(r["cold_share"])))
	if float(r["homeless_share"]) > 0.25:
		f.append("many people without beds (%d%% of the time)" % int(100 * float(r["homeless_share"])))
	if float(r["max_farms_per_person"]) > 1.0:
		f.append("more farms than people (%.1f per person)" % float(r["max_farms_per_person"]))
	if float(r["famine_share"]) > 0.2:
		f.append("food under a day's supply %d%% of the time" % int(100 * float(r["famine_share"])))
	for k in r["max_glut_days"]:
		if float(r["max_glut_days"][k]) > (30.0 if k in ["food", "water"] else 8.0):
			f.append("%s glut" % k)
	if float(r["failed_share"]) > 0.02:
		f.append("many failed decisions")
	if float(r["critical_share"]) > 0.05:
		f.append("people often in danger")
	return f


## Who is known for what at the end: {"woodcutter": 3, ..., "none": 5} (adults only).
func _roles() -> Dictionary:
	var out := {}
	for c: Colonist in sim.colonists.values():
		if c.is_adult():
			var r := sim.role_name(c.role) if c.role != "" else "none"
			out[r] = int(out.get(r, 0)) + 1
	return out


func _hall_seats() -> int:
	for b: Building in sim.buildings.values():
		if b.provides("meeting_place") != null and b.complete:
			return b.seats()
	return 0
