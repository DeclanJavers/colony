class_name UtilityBrain
extends Brain
## Hand-tuned baseline brain. Runs instantly with no Python, so the sim can be tested
## headless and Laya's choices can be compared against something sensible.

const RESOURCE_TARGETS := {"food": 80.0, "water": 40.0, "wood": 60.0, "stone": 50.0}
const KIND_OF := {
	"gather_wood": "gather", "gather_stone": "gather", "gather_food": "gather", "fetch_water": "gather",
	"farm": "work", "build": "build", "socialize": "socialize", "have_child": "socialize",
	"rest": "rest", "sleep": "rest", "check_stores": "work", "share_news": "socialize", "ask_help": "socialize",
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
	var believed := c.knowledge.recall(res, sim.clock) if c.knowledge.knows(res, sim.clock) else (target + extra) * 0.8
	return clampf(1.0 - believed / (target + extra), 0.0, 1.0)


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
			s = 0.35 + 0.8 * _scarcity(sim, c, "wood")
		"gather_stone":
			s = 0.3 + 0.8 * _scarcity(sim, c, "stone")
		"gather_food":
			s = 0.35 + 0.9 * _scarcity(sim, c, "food")
		"fetch_water":
			s = 0.3 + 0.8 * _scarcity(sim, c, "water")
		"farm":
			s = 1.0
		"build":
			var best := _best_build(sim)
			s = 0.0 if best < 0.2 else 0.3 + 0.5 * minf(best, 1.5)
		"socialize":
			s = 2.0 * _u(c, "social")
		"have_child":
			s = 1.1
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
		"check_stores":
			# Worth a look when what they know is old or missing.
			s = 1.3 * c.knowledge.uncertainty(sim.clock) - 0.35
	# Requests: doing what someone asked, weighted by how much they trust them and how recent it
	# is. Urgent needs still win (they score up to ~3), so a parched colonist drinks first.
	for r in sim.active_requests(c):
		if r["action"] == aid:
			var fresh := 1.0 - clampf((sim.clock - float(r["t"])) / Sim.REQUEST_TTL, 0.0, 1.0)
			s += 0.9 * float(r["trust"]) * fresh
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
	var s := 0.6 - 0.05 * float(sim.content.buildings[bid].get("tier", 0))
	match bid:
		"campfire":
			s += 1.0
		"well":
			s += 0.8 if sim.count_of("well") == 0 else -0.3
		"farm_plot":
			# Enough fields for the people there are: another one isn't needed (and costs wood
			# that houses need). The teacher's judgement, not a game rule.
			s += 0.7 if sim.count_of("farm_plot") < 1 + sim.alive_count() / 4 else -1.0
		"house":
			s += 0.9 if sim.free_beds() <= 1 else -0.6
		"stockpile", "storehouse":
			s += 0.9 if sim.stock_total() > sim.storage_capacity() * 0.8 else -0.4
		"workshop", "kitchen", "town_hall":
			s += 0.4
	return s
