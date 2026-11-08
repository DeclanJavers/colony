class_name Sim
extends RefCounted
## The whole colony simulation as plain data on a fixed tick. No rendering, no nodes.
## Views read it; brains decide for it; SimEvents broadcasts what happens.

const ACTION_TYPES_DIR := "res://scripts/sim/actions/types/"

var content: Content
var events := SimEvents.new()
var rng := RandomNumberGenerator.new()
var seed_value: int = 0
var map := WorldMap.new()
var clock: float = 0.0                  # game minutes since day 0 00:00

var colonists: Dictionary = {}          # id -> Colonist (alive only)
var graveyard: Array[Colonist] = []
var buildings: Dictionary = {}          # id -> Building
var nodes: Dictionary = {}              # id -> ResNode
var stock: Dictionary = {}              # resource -> amount (shared by everyone)
var unlocked_tier: int = 0

var brains: Dictionary = {}             # brain id -> Brain
var default_brain_id: String = "utility"
var describer: StateDescriber = StateDescriber.new()
var action_types: Dictionary = {}       # type name -> Script

var stats := {"births": 0, "deaths": {}, "decisions": 0, "failed_decisions": 0}

var _next_id: int = 1
var _next_request_id: int = 1
var _last_hour: int = -1
var _last_day: int = -1
var _used_names: Dictionary = {}


## ---- Setup ----------------------------------------------------------------

func setup(p_seed: int) -> void:
	content = Content.get_instance()
	seed_value = p_seed
	rng.seed = p_seed
	_load_action_types()
	map.setup(int(content.cfg("map_size", 60)))
	map.generate(rng, self)
	clock = 6.0 * 60.0  # start at 06:00 on day 0
	_last_hour = hour()
	_last_day = day()
	for r in content.resources:
		stock[r] = 0.0
	var start: Dictionary = content.cfg("start", {})
	for btype in start.get("buildings", []):
		var b := place_site(btype, null, true)
		if b != null:
			complete_building(b, null, true)
	for r in start.get("stock", {}):
		stock[r] = float(start["stock"][r])
	var ages: Array = content.cfg("founder_age_days", [5.0, 12.0])
	for i in int(start.get("colonists", 5)):
		var cell := map.nearest_walkable(map.center + Vector2i(rng.randi_range(-3, 3), rng.randi_range(-3, 3)))
		spawn_colonist(cell, rng.randf_range(float(ages[0]), float(ages[1])), [])
	for c: Colonist in colonists.values():
		observe_stock(c)   # the founders packed the stores themselves
	_update_tier(true)
	_assign_beds()


func register_brain(brain: Brain, make_default: bool = false) -> void:
	brains[brain.id] = brain
	if make_default or brains.size() == 1:
		default_brain_id = brain.id


func brain_for(c: Colonist) -> Brain:
	var bid := c.brain_id if c.brain_id != "" else default_brain_id
	return brains.get(bid, brains.get(default_brain_id)) as Brain


func _load_action_types() -> void:
	action_types.clear()
	var dir := DirAccess.open(ACTION_TYPES_DIR)
	if dir == null:
		push_error("Sim: missing %s" % ACTION_TYPES_DIR)
		return
	for f in dir.get_files():
		# Exported builds list .gd.remap / .gdc instead of .gd
		var file := f.trim_suffix(".remap")
		if not (file.ends_with(".gd") or file.ends_with(".gdc")):
			continue
		var type_name := file.get_basename()
		action_types[type_name] = load(ACTION_TYPES_DIR + type_name + ".gd")


func make_action(c: Colonist, def: Dictionary) -> ActionBase:
	var script: Script = action_types.get(def.get("type", ""))
	if script == null:
		push_error("Sim: unknown action type '%s'" % def.get("type", ""))
		return null
	return (script.new() as ActionBase).setup(self, c, def)


## ---- Time -----------------------------------------------------------------

func day() -> int:
	return int(clock / 1440.0)


func hour() -> int:
	return int(fmod(clock, 1440.0) / 60.0)


func time_of_day() -> float:
	## 0..24 float
	return fmod(clock, 1440.0) / 60.0


func is_night() -> bool:
	var nh: Array = content.cfg("night_hours", [21, 6])
	var h := hour()
	return h >= int(nh[0]) or h < int(nh[1])


func night_mult() -> float:
	return float(content.cfg("night_work_mult", 0.75)) if is_night() else 1.0


## ---- Main tick ------------------------------------------------------------

func tick(dt: float = 1.0) -> void:
	clock += dt
	var to_bury: Array[Colonist] = []
	for c: Colonist in colonists.values():
		if not c.alive():
			continue
		_update_needs(c, dt)
		if c.alive():
			_age(c, dt)
		if not c.alive():
			to_bury.append(c)
			continue
		if c.state == "acting" and c.action != null:
			var st := c.action.tick(dt)
			if st != ActionBase.RUNNING:
				_finish_action(c, st == ActionBase.DONE)
		if c.state == "idle" and clock >= c.hesitate_until:
			_request_decision(c)
	for c in to_bury:
		colonists.erase(c.id)
		graveyard.append(c)
	_world_tick(dt)
	if int(clock) % PERCEIVE_EVERY == 0:
		_perceive()
	if int(clock) % 30 == 0:
		_casual_chats()
	if hour() != _last_hour:
		_last_hour = hour()
		_on_hour()
		events.hour_passed.emit(day(), hour())
	if day() != _last_day:
		_last_day = day()
		events.day_passed.emit(day())
	events.ticked.emit(clock)


func _on_hour() -> void:
	for n: ResNode in nodes.values():
		var regrow := float(n.def().get("regrow_per_hour", 0))
		if regrow > 0.0:
			n.amount = minf(n.max_amount, n.amount + regrow)
	_assign_beds()


func _world_tick(dt: float) -> void:
	for b: Building in buildings.values():
		if b.complete and b.farm_stage == "growing":
			var f: Dictionary = b.provides("farm", {})
			b.farm_growth += dt / (float(f.get("grow_hours", 36)) * 60.0)
			if b.farm_growth >= 1.0:
				b.farm_growth = 1.0
				b.farm_stage = "ripe"
	for c: Colonist in colonists.values():
		if c.pregnant_until > 0.0 and clock >= c.pregnant_until:
			_give_birth(c)


## ---- Needs, aging, death ---------------------------------------------------

func _update_needs(c: Colonist, dt: float) -> void:
	var child_mult := 1.0 if c.is_adult() else float(content.cfg("child_need_decay_mult", 0.7))
	for need_id in content.needs:
		var nd: Dictionary = content.needs[need_id]
		var rate := float(nd.get("decay_per_hour", 1.0)) / 60.0 * dt * child_mult
		rate *= c.trait_mod(need_id + "_decay")
		if c.action != null:
			rate *= c.action.need_decay_mult(need_id)
		var v := maxf(0.0, float(c.needs.get(need_id, 100.0)) - rate)
		c.needs[need_id] = v
		if v <= 0.0:
			var was := float(c.zero_minutes.get(need_id, 0.0))
			if was == 0.0 and nd.get("death_after_hours_at_zero") != null:
				add_memory(c, "I am %s." % _need_word(need_id, 0.0), 6.0)
			c.zero_minutes[need_id] = was + dt
			var limit: Variant = nd.get("death_after_hours_at_zero")
			if limit != null and c.zero_minutes[need_id] >= float(limit) * 60.0:
				kill(c, str(nd.get("death_cause", "lack of " + str(nd.get("label", need_id)))))
				return
		else:
			c.zero_minutes[need_id] = 0.0


func _age(c: Colonist, dt: float) -> void:
	var was_adult := c.is_adult()
	c.age_minutes += dt
	if not was_adult and c.is_adult():
		add_memory(c, "I grew up and can work now.", 6.0)
		events.colonist_grew_up.emit(c)
	if c.age_days() >= c.lifespan_days:
		kill(c, "old age")


func kill(c: Colonist, cause: String) -> void:
	if not c.alive():
		return
	if c.action != null:
		c.action.end(false)
		c.action = null
	c.state = "dead"
	c.cause_of_death = cause
	c.died_at = clock
	c.pregnant_until = -1.0
	stats["deaths"][cause] = int(stats["deaths"].get(cause, 0)) + 1
	for other: Colonist in colonists.values():
		if other == c or not other.alive():
			continue
		var close := other.partner_id == c.id or c.parent_ids.has(other.id) or other.parent_ids.has(c.id)
		add_memory(other, "%s died of %s." % [c.name, cause], 9.0 if close else 4.0)
		if other.partner_id == c.id:
			other.partner_id = -1
	events.colonist_died.emit(c, cause)


func _need_word(need_id: String, v: float) -> String:
	var nd: Dictionary = content.needs.get(need_id, {})
	var th: Array = nd.get("thresholds", [10, 30, 55, 80])
	var words: Array = nd.get("words", ["very low", "low", "ok", "good", "great"])
	var i := 0
	while i < th.size() and v >= float(th[i]):
		i += 1
	return str(words[mini(i, words.size() - 1)])


func need_word(c: Colonist, need_id: String) -> String:
	return _need_word(need_id, float(c.needs.get(need_id, 100.0)))


## ---- Colonists ------------------------------------------------------------

func spawn_colonist(cell: Vector2i, age_days: float, parents: Array) -> Colonist:
	var c := Colonist.new()
	c.id = _new_id()
	c.knowledge = Knowledge.new(c.id)
	c.name = _pick_name()
	c.age_minutes = age_days * 1440.0
	var ls: Array = content.cfg("lifespan_days", [25.0, 40.0])
	c.lifespan_days = rng.randf_range(float(ls[0]), float(ls[1]))
	c.parent_ids = parents.duplicate()
	var trait_ids: Array = content.traits.keys()
	var inherited: Array = []
	for pid in parents:
		var p := _find_colonist_any(int(pid))
		if p != null and not p.traits.is_empty():
			inherited.append(p.traits[rng.randi() % p.traits.size()])
	while c.traits.size() < 2:
		var t: String = inherited.pop_back() if not inherited.is_empty() and rng.randf() < 0.6 else trait_ids[rng.randi() % trait_ids.size()]
		if not c.traits.has(t) and not _conflicting_trait(c.traits, t):
			c.traits.append(t)
	for need_id in content.needs:
		c.needs[need_id] = rng.randf_range(75.0, 100.0)
		c.zero_minutes[need_id] = 0.0
	c.cell = cell
	c.pos = Vector2(cell)
	colonists[c.id] = c
	if not parents.is_empty():
		stats["births"] += 1
		add_memory(c, "I was born.", 5.0)
	events.colonist_born.emit(c)
	return c


func _conflicting_trait(have: Array[String], t: String) -> bool:
	var pairs := [["hardworking", "lazy"], ["social", "loner"]]
	for p in pairs:
		if p.has(t):
			for other in p:
				if other != t and have.has(other):
					return true
	return false


func _pick_name() -> String:
	var pool: Array = content.names
	for i in 200:
		var n: String = pool[rng.randi() % pool.size()]
		if not _used_names.has(n):
			_used_names[n] = 1
			return n
	var base: String = pool[rng.randi() % pool.size()]
	_used_names[base] = int(_used_names.get(base, 1)) + 1
	return "%s %d" % [base, _used_names[base]]


## ---- Knowledge: what colonists see, hear and are asked -----------------------------

const PERCEIVE_EVERY := 15   # sim minutes between "looking around"
const SIGHT := 3.0           # tiles: close enough to a storage pile to see what's in it


## The colonist sees the shared stores right now.
func observe_stock(c: Colonist) -> void:
	for k in Knowledge.STOCK_KEYS:
		c.knowledge.observe(k, float(stock.get(k, 0.0)), clock)


const CHAT_NEAR := 1.8          # tiles: close enough to chat while working
const CHAT_COOLDOWN := 180.0    # minutes between a colonist's casual chats
const CHAT_CHANCE := 0.35       # per check, for a pair that's close and free to chat


## Colonists working side by side sometimes swap a few words (and news) without stopping.
func _casual_chats() -> void:
	var busy := colonists.values().filter(func(c: Colonist) -> bool:
		return c.state == "acting" and c.action != null and c.action.phase != "walking" and c.action.phase != "sleeping" \
			and clock - float(c.get_meta("last_chat", -1e9)) > CHAT_COOLDOWN)
	for i in busy.size():
		var a: Colonist = busy[i]
		if clock - float(a.get_meta("last_chat", -1e9)) <= CHAT_COOLDOWN:
			continue
		for j in range(i + 1, busy.size()):
			var b: Colonist = busy[j]
			if clock - float(b.get_meta("last_chat", -1e9)) <= CHAT_COOLDOWN or a.pos.distance_to(b.pos) > CHAT_NEAR:
				continue
			if rng.randf() > CHAT_CHANCE:
				continue
			a.set_meta("last_chat", clock)
			b.set_meta("last_chat", clock)
			converse(a, b, "chat")
			break


func _perceive() -> void:
	var stores: Array[Building] = []
	for b: Building in buildings.values():
		if b.complete and float(b.provides("storage", 0)) > 0.0:
			stores.append(b)
	for c: Colonist in colonists.values():
		for b in stores:
			if c.pos.distance_to(b.center()) <= SIGHT + b.size.x * 0.5:
				observe_stock(c)
				break


## How much `c` believes what `from` tells them (0..1): liking someone makes them more believable.
func trust(c: Colonist, from: Colonist) -> float:
	if from == null:
		return 0.5
	var t := 0.35 + float(c.affection.get(from.id, 0.0)) / 120.0
	if c.partner_id == from.id or c.parent_ids.has(from.id):
		t += 0.25
	return clampf(t, 0.1, 1.0)


## Someone asked `c` to do an action. Kept as a dated request (and a memory) that brains read.
func add_request(c: Colonist, from_name: String, action_id: String, trust_v: float) -> void:
	var def: Dictionary = content.actions_by_id.get(action_id, {})
	if def.is_empty():
		return
	c.requests = c.requests.filter(func(r) -> bool: return r["action"] != action_id and clock - float(r["t"]) < REQUEST_TTL)
	c.requests.append({"from": from_name, "action": action_id, "label": def.get("label", action_id), "t": clock, "trust": trust_v})
	if c.requests.size() > 3:
		c.requests.pop_front()
	add_memory(c, "%s asked me to %s." % [from_name, def.get("label", action_id)], 3.0 + 4.0 * trust_v)


const REQUEST_TTL := 24.0 * 60.0
const GATHER_FOR := {"food": "gather_food", "water": "fetch_water", "wood": "gather_wood", "stone": "gather_stone"}
const SHORT_BELOW := {"food": 40.0, "water": 20.0, "wood": 25.0, "stone": 25.0}

## Optional: called with each conversation dict. Return true to take it over (e.g. the game asks
## the chat model to write the dialogue) and later call apply_conversation(conv, result).
## Unset (headless, training) = a quick symbolic exchange of facts happens immediately.
var conversation_handler: Callable
var _conv_id := 0


## What `c` believes the colony is shortest of, or "" if nothing looks short to them.
func believed_shortage(c: Colonist) -> String:
	var worst := ""
	var worst_ratio := 1.0
	for k in Knowledge.STOCK_KEYS:
		if not c.knowledge.knows(k, clock):
			continue
		var ratio := c.knowledge.recall(k, clock) / float(SHORT_BELOW[k])
		if ratio < worst_ratio:
			worst_ratio = ratio
			worst = k
	return worst


## Two colonists talk. intent: "chat" (socializing), "news" (came to share news), "ask" (came to
## ask for help with `topic`, an action id).
func converse(a: Colonist, b: Colonist, intent: String, topic: String = "") -> void:
	_conv_id += 1
	var conv := {"id": _conv_id, "t": clock, "a": a, "b": b, "intent": intent, "topic": topic}
	events.conversation_started.emit(conv)
	if conversation_handler.is_valid() and bool(conversation_handler.call(conv)):
		return
	apply_conversation(conv, symbolic_conversation(conv))


## The no-LLM version: each says what they know (as they remember it), plus the ask if any.
func symbolic_conversation(conv: Dictionary) -> Dictionary:
	var a: Colonist = conv["a"]
	var b: Colonist = conv["b"]
	var facts := []
	var lines := []
	for pair in [[a, b], [b, a]]:
		var sp: Colonist = pair[0]
		var heard: Colonist = pair[1]
		for k in Knowledge.STOCK_KEYS:
			if sp.knowledge.knows(k, clock):
				facts.append({"from": sp.name, "to": heard.name, "key": k, "value": sp.knowledge.tell(k, clock),
					"t_source": float(sp.knowledge.facts[k]["t"])})
	var requests := []
	if conv["intent"] == "ask" and conv["topic"] != "":
		requests.append({"from": a.name, "to": b.name, "action": conv["topic"]})
		lines.append({"who": a.name, "text": "Could you %s?" % content.actions_by_id.get(conv["topic"], {}).get("label", conv["topic"])})
	elif conv["intent"] == "news":
		var short := believed_shortage(a)
		lines.append({"who": a.name, "text": ("We're running low on %s." % short) if short != "" else "I just checked the stores."})
	return {"lines": lines, "facts": facts, "requests": requests, "llm": false}


## Apply what was said: listeners update beliefs (by trust), take on requests, remember notable news.
func apply_conversation(conv: Dictionary, result: Dictionary) -> void:
	var a: Colonist = conv["a"]
	var b: Colonist = conv["b"]
	var by_name := {a.name: a, b.name: b}
	var learned := []
	for f in result.get("facts", []):
		var sp: Colonist = by_name.get(f.get("from", ""))
		var li: Colonist = by_name.get(f.get("to", ""))
		if sp == null or li == null or sp == li or not li.alive() or not Knowledge.STOCK_KEYS.has(f.get("key", "")):
			continue
		var v := float(f.get("value", -1))
		if v < 0.0:
			continue
		var changed := li.knowledge.hear(f["key"], v, float(f.get("t_source", clock)), sp.name, trust(li, sp), clock)
		if changed:
			learned.append({"who": li.name, "key": f["key"], "value": v, "from": sp.name})
			add_memory(li, "%s told me the stores have about %d %s." % [sp.name, int(v), f["key"]], 2.5)
	for r in result.get("requests", []):
		var sp: Colonist = by_name.get(r.get("from", ""))
		var li: Colonist = by_name.get(r.get("to", ""))
		if sp != null and li != null and sp != li and li.alive():
			add_request(li, sp.name, String(r.get("action", "")), trust(li, sp))
	conv["lines"] = result.get("lines", [])
	conv["facts"] = result.get("facts", [])
	conv["requests"] = result.get("requests", [])
	conv["learned"] = learned
	conv["llm"] = result.get("llm", false)
	for c in [a, b]:
		c.conversations.append({"t": conv["t"], "with": (b if c == a else a).name, "intent": conv["intent"],
			"lines": conv["lines"], "learned": learned.filter(func(x) -> bool: return x["who"] == c.name)})
		if c.conversations.size() > 5:
			c.conversations.pop_front()
	events.conversation_finished.emit(conv)


## Cells other colonists are standing on (when not moving) or heading to.


func active_requests(c: Colonist) -> Array:
	return c.requests.filter(func(r) -> bool: return clock - float(r["t"]) < REQUEST_TTL)


## Cells other colonists are standing on (when not moving) or heading to.
func claimed_cells(except: Colonist) -> Dictionary:
	var out := {}
	for o: Colonist in colonists.values():
		if o == except:
			continue
		if o.action != null and o.action.goal_cell.x >= 0:
			out[o.action.goal_cell] = true
		if o.action == null or o.action.path.is_empty():
			out[o.cell] = true
	return out


## `cell` if nobody else has claimed it, else the nearest free walkable cell around it that the
## colonist can reach (rings 1-2). Falls back to `cell` when everything nearby is taken.
func free_spot_near(cell: Vector2i, c: Colonist) -> Vector2i:
	var taken := claimed_cells(c)
	if not taken.has(cell):
		return cell
	for ring in [1, 2]:
		var best := Vector2i(-1, -1)
		var best_d := INF
		for dy in range(-ring, ring + 1):
			for dx in range(-ring, ring + 1):
				if maxi(absi(dx), absi(dy)) != ring:
					continue
				var n := cell + Vector2i(dx, dy)
				if taken.has(n) or not map.is_walkable(n):
					continue
				var d := Vector2(n - c.cell).length_squared()
				if d < best_d and (n == c.cell or not map.find_path(c.cell, n).is_empty()):
					best_d = d
					best = n
		if best.x >= 0:
			return best
	return cell


## What's on a map cell, for inspecting (null if nothing).
func building_at_cell(cell: Vector2i) -> Building:
	if map.building_at.has(cell):
		return buildings.get(map.building_at[cell])
	for b: Building in buildings.values():   # walkable buildings (farms) aren't in building_at
		if b.cells().has(cell):
			return b
	return null


func node_at_cell(cell: Vector2i) -> ResNode:
	return nodes.get(map.node_at.get(cell, -1)) as ResNode


## Living or dead colonist by id (null if unknown).
func find_colonist(cid: int) -> Colonist:
	return _find_colonist_any(cid)


func _find_colonist_any(cid: int) -> Colonist:
	if colonists.has(cid):
		return colonists[cid]
	for g in graveyard:
		if g.id == cid:
			return g
	return null


func alive_count() -> int:
	return colonists.size()


func add_memory(c: Colonist, text: String, importance: float) -> void:
	var m := c.remember(clock, "Day %d: %s" % [day(), text], importance)
	events.memory_added.emit(c, m)


func move_speed(c: Colonist) -> float:
	var s := float(content.cfg("move_speed_tiles_per_min", 0.4)) * c.trait_mod("move_speed")
	s *= exhaustion_mult(c, "move_speed")
	if not c.is_adult():
		s *= 0.8
	if is_night():
		s *= 0.85
	return s


func exhaustion_mult(c: Colonist, key: String) -> float:
	if float(c.needs.get("energy", 100.0)) > 0.0:
		return 1.0
	return float(content.needs.get("energy", {}).get("zero_effect", {}).get(key, 1.0))


func ensure_walkable(c: Colonist) -> void:
	if not map.is_walkable(c.cell):
		c.cell = map.nearest_walkable(c.cell)
		c.pos = Vector2(c.cell)


func social_candidates(c: Colonist) -> Array[Colonist]:
	var out: Array[Colonist] = []
	for o: Colonist in colonists.values():
		if o != c and o.alive():
			out.append(o)
	out.sort_custom(func(a: Colonist, b: Colonist) -> bool:
		return c.pos.distance_squared_to(a.pos) < c.pos.distance_squared_to(b.pos))
	return out


func related(a: Colonist, b: Colonist) -> bool:
	if a.parent_ids.has(b.id) or b.parent_ids.has(a.id):
		return true
	for p in a.parent_ids:
		if b.parent_ids.has(p):
			return true
	return false


func finish_social(c: Colonist, other: Colonist, params: Dictionary) -> void:
	var gain := colony_mult("social_gain")
	var restore := float(params.get("restore", 40)) * gain
	c.needs["social"] = minf(100.0, float(c.needs.get("social", 0)) + restore)
	other.needs["social"] = minf(100.0, float(other.needs.get("social", 0)) + restore * 0.5)
	var aff := float(params.get("affection_gain", 8)) * gain
	c.affection[other.id] = clampf(float(c.affection.get(other.id, 0.0)) + aff, -100.0, 100.0)
	other.affection[c.id] = clampf(float(other.affection.get(c.id, 0.0)) + aff * 0.8, -100.0, 100.0)
	add_memory(c, "I spent time with %s." % other.name, 1.5)
	_check_couple(c, other)


func _check_couple(a: Colonist, b: Colonist) -> void:
	if not (a.is_adult() and b.is_adult()) or a.partner_id != -1 or b.partner_id != -1 or related(a, b):
		return
	var need := float(content.cfg("reproduction.couple_affection", 60))
	if float(a.affection.get(b.id, 0)) >= need and float(b.affection.get(a.id, 0)) >= need:
		a.partner_id = b.id
		b.partner_id = a.id
		add_memory(a, "%s and I became a couple." % b.name, 8.0)
		add_memory(b, "%s and I became a couple." % a.name, 8.0)
		events.couple_formed.emit(a, b)


func free_beds() -> int:
	var beds := 0
	for b: Building in buildings.values():
		if b.complete:
			beds += int(b.provides("beds", 0))
	var expecting := 0
	for c: Colonist in colonists.values():
		if c.pregnant_until > 0.0:
			expecting += 1
	return beds - alive_count() - expecting


func can_have_child(c: Colonist) -> bool:
	if not c.is_adult() or c.partner_id < 0 or c.pregnant_until > 0.0:
		return false
	var p := colonists.get(c.partner_id) as Colonist
	if p == null or not p.alive() or not p.is_adult() or p.pregnant_until > 0.0:
		return false
	var cooldown := float(content.cfg("reproduction.cooldown_days", 2)) * 1440.0
	if clock - c.last_child_at < cooldown or clock - p.last_child_at < cooldown:
		return false
	if alive_count() >= int(content.cfg("max_population", 50)):
		return false
	if free_beds() <= 0:
		return false
	return float(stock.get("food", 0)) >= float(content.cfg("reproduction.min_food_stock", 30))


func start_pregnancy(c: Colonist) -> void:
	c.pregnant_until = clock + float(content.cfg("reproduction.pregnancy_hours", 24)) * 60.0
	c.last_child_at = clock
	var p := colonists.get(c.partner_id) as Colonist
	if p != null:
		p.last_child_at = clock
		add_memory(p, "%s and I are expecting a child." % c.name, 8.0)
	add_memory(c, "I am expecting a child.", 8.0)


func _give_birth(c: Colonist) -> void:
	c.pregnant_until = -1.0
	var parents := [c.id]
	if c.partner_id >= 0:
		parents.append(c.partner_id)
	var baby := spawn_colonist(map.nearest_walkable(c.cell), 0.0, parents)
	baby.affection[c.id] = 50.0
	c.affection[baby.id] = 60.0
	add_memory(c, "My child %s was born." % baby.name, 9.0)
	var p := colonists.get(c.partner_id) as Colonist
	if p != null:
		p.affection[baby.id] = 60.0
		baby.affection[p.id] = 50.0
		add_memory(p, "My child %s was born." % baby.name, 9.0)
	_assign_beds()


func _assign_beds() -> void:
	var houses: Array[Building] = []
	for b: Building in buildings.values():
		if b.complete and int(b.provides("beds", 0)) > 0:
			houses.append(b)
			for i in b.beds.size():
				var occ := int(b.beds[i])
				if occ >= 0 and not colonists.has(occ):
					b.beds[i] = -1
	for c: Colonist in colonists.values():
		if c.home_id >= 0:
			var h := buildings.get(c.home_id) as Building
			if h == null or not h.beds.has(c.id):
				c.home_id = -1
	for c: Colonist in colonists.values():
		if c.home_id >= 0:
			continue
		# Prefer the house where a partner or parent already sleeps.
		var best: Building = null
		for h in houses:
			if not h.beds.has(-1):
				continue
			if best == null:
				best = h
			if h.beds.has(c.partner_id) or (not c.parent_ids.is_empty() and h.beds.has(c.parent_ids[0])):
				best = h
				break
		if best == null:
			break
		best.beds[best.beds.find(-1)] = c.id
		c.home_id = best.id


## ---- Decisions ------------------------------------------------------------

func available_actions(c: Colonist) -> Array[Dictionary]:
	var out: Array[Dictionary] = []
	for def: Dictionary in content.actions:
		if not def.get("for", ["adult"]).has(c.life_stage()):
			continue
		var a := make_action(c, def)
		if a != null and a.available():
			out.append(def)
	return out


func build_context(c: Colonist) -> Dictionary:
	var opts := []
	var has_build := false
	var has_social := false
	for def in available_actions(c):
		opts.append({"id": def["id"], "label": def["label"], "description": def["description"]})
		has_build = has_build or def["type"] == "build"
		has_social = has_social or def["type"] in ["socialize", "inform"]
	var bopts := []
	if has_build:
		for t in build_options():
			if t == "continue":
				var site := nearest_site(c.cell)
				var lbl: String = site.def().get("label", site.type)
				bopts.append({"id": "continue", "label": "finish the %s" % lbl,
					"description": "help finish the half-built %s (%d%% done)" % [lbl, int(site.progress() * 100)]})
			else:
				var d: Dictionary = content.buildings[t]
				bopts.append({"id": t, "label": "new %s" % d.get("label", t), "description": d.get("description", "")})
	var sopts := []
	if has_social:
		var seen := {}
		for o in social_candidates(c).slice(0, int(content.cfg("decision.max_social_candidates", 6))):
			var lbl: String = o.name if not seen.has(o.name) else "%s (#%d)" % [o.name, o.id]
			seen[o.name] = true
			sopts.append({"id": str(o.id), "label": lbl, "description": describer.relation_text(self, c, o)})
	var rid := _next_request_id
	_next_request_id += 1
	return {
		"request_id": rid, "colonist_id": c.id, "colonist_name": c.name,
		"clock": clock, "day": day(), "hour": hour(),
		"state_text": describer.describe(self, c), "describer_version": describer.VERSION,
		"options": opts, "build_options": bopts, "social_options": sopts,
	}


func _request_decision(c: Colonist) -> void:
	var brain := brain_for(c)
	if brain == null:
		return
	var ctx := build_context(c)
	if (ctx["options"] as Array).is_empty():
		c.hesitate_until = clock + 10.0
		return
	c.state = "thinking"
	c.thinking_since = clock
	c.pending_request_id = int(ctx["request_id"])
	events.decision_requested.emit(c, ctx)
	brain.decide(ctx, self, _on_decision.bind(c.id, ctx))


func _on_decision(decision: Dictionary, cid: int, ctx: Dictionary) -> void:
	var c := colonists.get(cid) as Colonist
	if c == null or not c.alive() or c.pending_request_id != int(ctx["request_id"]):
		return
	c.pending_request_id = -1
	stats["decisions"] += 1
	var def: Dictionary = content.actions_by_id.get(decision.get("action", ""), {})
	var began := false
	if not def.is_empty():
		var a := make_action(c, def)
		if a != null:
			a.extra = decision
			a.started_at = clock
			if a.available() and a.begin():
				c.action = a
				c.state = "acting"
				began = true
				events.action_started.emit(c, a)
	if not began:
		# A choice that can't be carried out costs time, doubling on repeats (max 2h).
		# This isn't an override: the brain still makes the next choice.
		stats["failed_decisions"] += 1
		c.fail_streak += 1
		c.state = "idle"
		var wait := float(content.cfg("decision.hesitate_minutes_on_fail", 5)) * pow(2.0, mini(c.fail_streak - 1, 5))
		c.hesitate_until = clock + minf(wait, 120.0)
	else:
		c.fail_streak = 0
	c.last_decision = {"ctx": ctx, "decision": decision, "began": began, "t": clock}
	var act_id := String(decision.get("action", ""))
	var lbl := act_id
	for o in ctx["options"]:
		if o["id"] == act_id:
			lbl = o["label"]
	c.recent.append({"t": clock, "action": act_id, "label": lbl, "began": began,
		"p": float(decision.get("probs", {}).get(act_id, 0.0)), "brain": decision.get("brain", "")})
	if c.recent.size() > 8:
		c.recent.pop_front()
	events.decision_made.emit(c, ctx, decision, began)


func _finish_action(c: Colonist, ok: bool) -> void:
	var a := c.action
	a.end(ok)
	c.action = null
	c.state = "idle"
	events.action_finished.emit(c, a, ok)


func oldest_thinking_minutes() -> float:
	var worst := 0.0
	for c: Colonist in colonists.values():
		if c.state == "thinking":
			worst = maxf(worst, clock - c.thinking_since)
	return worst


## ---- Stock & storage ------------------------------------------------------

func storage_capacity() -> float:
	var cap := 0.0
	for b: Building in buildings.values():
		if b.complete:
			cap += float(b.provides("storage", 0))
	return cap


func stock_total() -> float:
	var t := 0.0
	for r in stock:
		t += float(stock[r])
	return t


func has_storage_space() -> bool:
	return stock_total() < storage_capacity() - 1.0


func deposit(res: String, amount: float, by: Colonist = null) -> float:
	var put := minf(amount, maxf(0.0, storage_capacity() - stock_total()))
	stock[res] = float(stock.get(res, 0.0)) + put
	events.stock_changed.emit(stock)
	if by != null:
		observe_stock(by)   # standing at the pile, they see what's there
		if put > 0.0:
			events.delivered.emit(by, res, put)
	return put


func take(res: String, amount: float) -> bool:
	if float(stock.get(res, 0.0)) < amount:
		return false
	stock[res] = float(stock[res]) - amount
	events.stock_changed.emit(stock)
	return true


func affordable(cost: Dictionary) -> bool:
	for r in cost:
		if float(stock.get(r, 0.0)) < float(cost[r]):
			return false
	return true


func storage_access(from: Vector2i) -> Vector2i:
	var best := Vector2i(-1, -1)
	var best_d := INF
	for b: Building in buildings.values():
		if b.complete and float(b.provides("storage", 0)) > 0.0:
			var a := map.access_cell(b.cells(), from)
			var d := Vector2(a - from).length_squared()
			if a.x >= 0 and d < best_d:
				best_d = d
				best = a
	return best


## ---- Sources (where to eat, drink, gather) --------------------------------

func find_sources(specs: Array, resource: String, from: Vector2i, amount: float, for_gather: bool = false) -> Array:
	var cands := []
	for spec: String in specs:
		if spec == "stockpile":
			if for_gather or float(stock.get(resource, 0.0)) < amount:
				continue
			for b: Building in buildings.values():
				if b.complete and float(b.provides("storage", 0)) > 0.0:
					cands.append({"kind": "stockpile", "ref": b, "cells": b.cells(), "d": Vector2(b.cell - from).length_squared()})
		elif spec.begins_with("node:"):
			var t := spec.substr(5)
			for n: ResNode in nodes.values():
				if n.type == t and n.amount >= 1.0 and (not for_gather or n.workers < 2):
					cands.append({"kind": "node", "ref": n, "cells": [n.cell], "d": Vector2(n.cell - from).length_squared()})
		elif spec.begins_with("building:"):
			var t := spec.substr(9)
			for b: Building in buildings.values():
				if b.complete and b.type == t:
					cands.append({"kind": "building", "ref": b, "cells": b.cells(), "d": Vector2(b.cell - from).length_squared()})
		elif spec == "tile:water":
			for s in map.shore_cells:
				cands.append({"kind": "tile", "ref": s, "cells": [s], "d": Vector2(s - from).length_squared()})
	cands.sort_custom(func(a, b) -> bool: return a["d"] < b["d"])
	var out := []
	for cand in cands.slice(0, 4):
		var typed: Array[Vector2i] = []
		typed.assign(cand["cells"])
		var acc := map.access_cell(typed, from)
		if acc.x >= 0:
			cand["access"] = acc
			out.append(cand)
	return out


func source_def(source: Dictionary) -> Dictionary:
	if source.get("kind") == "node":
		return (source["ref"] as ResNode).def()
	return {}


func reserve_source(source: Dictionary, delta: int) -> void:
	if source.get("kind") == "node":
		var n := source["ref"] as ResNode
		n.workers = maxi(0, n.workers + delta)


func take_from_source(source: Dictionary, resource: String, amount: float) -> bool:
	match source.get("kind"):
		"stockpile":
			return take(resource, amount)
		"node":
			var n := source["ref"] as ResNode
			if not nodes.has(n.id) or n.amount < 1.0:
				return false
			n.amount -= minf(amount, n.amount)
			if n.amount <= 0.0:
				_deplete(n)
			return true
	return true


func harvest_source(source: Dictionary, amount: float) -> float:
	if source.get("kind") == "node":
		var n := source["ref"] as ResNode
		if not nodes.has(n.id):
			return 0.0
		var got := minf(amount, n.amount)
		n.amount -= got
		if n.amount <= 0.0:
			_deplete(n)
		return got
	return amount


func spawn_node(type: String, cell: Vector2i) -> ResNode:
	var d: Dictionary = content.nodes[type]
	var n := ResNode.new()
	n.id = _new_id()
	n.type = type
	n.cell = cell
	n.max_amount = float(d.get("amount", 10))
	n.amount = n.max_amount
	nodes[n.id] = n
	if d.get("blocks", true):
		map.node_at[cell] = n.id
		map.refresh_cell(cell)
	return n


func _deplete(n: ResNode) -> void:
	if float(n.def().get("regrow_per_hour", 0)) > 0.0:
		return  # bushes stay and regrow
	nodes.erase(n.id)
	map.node_at.erase(n.cell)
	map.refresh_cell(n.cell)
	events.node_depleted.emit(n)


## ---- Buildings ------------------------------------------------------------

func count_of(type: String, include_sites: bool = true) -> int:
	var n := 0
	for b: Building in buildings.values():
		if b.type == type and (b.complete or include_sites):
			n += 1
	return n


func nearest_site(from: Vector2i) -> Building:
	var best: Building = null
	var best_d := INF
	for b: Building in buildings.values():
		if not b.complete:
			var d := Vector2(b.cell - from).length_squared()
			if d < best_d:
				best_d = d
				best = b
	return best


func nearest_building_with(key: String, from: Vector2i) -> Building:
	var best: Building = null
	var best_d := INF
	for b: Building in buildings.values():
		if b.complete and b.provides(key) != null:
			var d := Vector2(b.cell - from).length_squared()
			if d < best_d:
				best_d = d
				best = b
	return best


func build_options() -> Array[String]:
	## "continue" (help an unfinished site) + every unlocked, affordable, under-limit type.
	var out: Array[String] = []
	if nearest_site(map.center) != null:
		out.append("continue")
	for t in content.buildings:
		var d: Dictionary = content.buildings[t]
		if int(d.get("tier", 0)) > unlocked_tier:
			continue
		if count_of(t) >= int(d.get("max_count", 999)):
			continue
		if affordable(d.get("cost", {})):
			out.append(t)
	return out


func footprint_of(type: String) -> Vector2i:
	var fp: Array = content.buildings.get(type, {}).get("footprint", [1, 1])
	return Vector2i(int(fp[0]), int(fp[1]))


func find_build_spot(type: String, from: Vector2i) -> Vector2i:
	## First free spot (spiralling out from the centre) that `from` can actually walk to.
	var size := footprint_of(type)
	var checked := 0
	for origin in _spots(size):
		var cells: Array[Vector2i] = []
		for y in size.y:
			for x in size.x:
				cells.append(origin + Vector2i(x, y))
		if reach_cell(from, cells).x >= 0:
			return origin
		checked += 1
		if checked > 30:
			break
	return Vector2i(-1, -1)


func reach_cell(from: Vector2i, cells: Array[Vector2i]) -> Vector2i:
	## Access cell for `cells` that is reachable from `from`, or (-1,-1).
	var acc := map.access_cell(cells, from)
	if acc.x < 0:
		return acc
	if acc == from or not map.find_path(map.nearest_walkable(from), acc).is_empty() or map.nearest_walkable(from) == acc:
		return acc
	return Vector2i(-1, -1)


func nearest_reachable_site(from: Vector2i) -> Building:
	var sites: Array[Building] = []
	for b: Building in buildings.values():
		if not b.complete:
			sites.append(b)
	sites.sort_custom(func(a: Building, b: Building) -> bool:
		return Vector2(a.cell - from).length_squared() < Vector2(b.cell - from).length_squared())
	for s in sites:
		if reach_cell(from, s.cells()).x >= 0:
			return s
	return null


func place_site(type: String, by: Colonist, free: bool = false, spot: Vector2i = Vector2i(-1, -1)) -> Building:
	var d: Dictionary = content.buildings.get(type, {})
	if d.is_empty() or (not free and not affordable(d.get("cost", {}))):
		return null
	var size := footprint_of(type)
	if spot.x < 0:
		spot = _find_spot(size)
	if spot.x < 0:
		return null
	if not free:
		for r in d.get("cost", {}):
			stock[r] = float(stock[r]) - float(d["cost"][r])
		events.stock_changed.emit(stock)
	var b := Building.new()
	b.id = _new_id()
	b.type = type
	b.cell = spot
	b.size = size
	b.work_needed = float(d.get("work", 60))
	for i in int(d.get("provides", {}).get("beds", 0)):
		b.beds.append(-1)
	b.placed_at = clock
	b.placed_by = by.name if by != null else ""
	buildings[b.id] = b
	if not d.get("walkable", false):
		for cell in b.cells():
			map.building_at[cell] = b.id
			map.refresh_cell(cell)
	if by != null:
		add_memory(by, "I started building a %s." % d.get("label", type), 3.0)
	events.building_placed.emit(b)
	return b


func complete_building(b: Building, by: Colonist, quiet: bool = false) -> void:
	b.complete = true
	b.work_done = b.work_needed
	b.completed_at = clock
	var label: String = b.def().get("label", b.type)
	if not quiet:
		for c: Colonist in colonists.values():
			add_memory(c, ("I finished the %s." if c == by else "The colony finished a %s.") % label, 4.0 if c == by else 2.5)
	events.building_completed.emit(b)
	_update_tier(quiet)
	_assign_beds()


func _find_spot(size: Vector2i) -> Vector2i:
	for origin in _spots(size):
		return origin
	return Vector2i(-1, -1)


func _spots(size: Vector2i) -> Array[Vector2i]:
	## Every valid origin, spiralling out from the colony centre (nearest first).
	var out: Array[Vector2i] = []
	var c := map.center
	for r in range(0, map.size / 2):
		for dy in range(-r, r + 1):
			for dx in range(-r, r + 1):
				if maxi(absi(dx), absi(dy)) != r:
					continue
				var origin := c + Vector2i(dx, dy)
				if _fits(origin, size):
					out.append(origin)
		if out.size() >= 40:
			break
	return out


func _fits(origin: Vector2i, size: Vector2i) -> bool:
	## Clear footprint plus a 1-tile walkway ring (no buildings, trees or rocks in the ring),
	## so buildings never wall each other off.
	for y in range(-1, size.y + 1):
		for x in range(-1, size.x + 1):
			var cell := origin + Vector2i(x, y)
			if not map.in_bounds(cell):
				return false
			if map.building_at.has(cell) or map.node_at.has(cell):
				return false
			var inside := x >= 0 and y >= 0 and x < size.x and y < size.y
			if inside and map.tile(cell) != WorldMap.Tile.GRASS:
				return false
	# Walkable farm plots also must not overlap other walkable sites.
	for b: Building in buildings.values():
		if Rect2i(b.cell - Vector2i.ONE, b.size + Vector2i(2, 2)).intersects(Rect2i(origin, size)):
			return false
	return true


func find_farm_job(from: Vector2i) -> Building:
	var best: Building = null
	var best_d := INF
	for b: Building in buildings.values():
		if b.complete and b.provides("farm") != null and b.reserved_by == -1 and (b.farm_stage == "empty" or b.farm_stage == "ripe"):
			var d := Vector2(b.cell - from).length_squared()
			if d < best_d:
				best_d = d
				best = b
	return best


func colony_mult(key: String) -> float:
	var m := 1.0
	for b: Building in buildings.values():
		if b.complete:
			m *= float(b.provides("colony", {}).get(key, 1.0))
	return m


func _update_tier(quiet: bool = false) -> void:
	var best := unlocked_tier
	for t: Dictionary in content.tiers:
		var ok := true
		var req: Dictionary = t.get("requires", {})
		for btype in req:
			if count_of(btype, false) < int(req[btype]):
				ok = false
		if ok:
			best = maxi(best, int(t["tier"]))
		else:
			break
	if best > unlocked_tier:
		unlocked_tier = best
		if not quiet:
			var names: Array[String] = []
			for bt in content.buildings:
				if int(content.buildings[bt].get("tier", 0)) == best:
					names.append(content.buildings[bt].get("label", bt))
			for c: Colonist in colonists.values():
				add_memory(c, "We learned how to build: %s." % ", ".join(names), 5.0)
		events.tier_unlocked.emit(unlocked_tier)


## ---- Misc -----------------------------------------------------------------

func _new_id() -> int:
	var i := _next_id
	_next_id += 1
	return i


func summary() -> Dictionary:
	var built := {}
	for b: Building in buildings.values():
		if b.complete:
			built[b.type] = int(built.get(b.type, 0)) + 1
	var adults := 0
	for c: Colonist in colonists.values():
		if c.is_adult():
			adults += 1
	var st := {}
	for r in stock:
		st[r] = int(stock[r])
	return {"day": day(), "hour": hour(), "pop": alive_count(), "adults": adults,
		"children": alive_count() - adults, "births": stats["births"], "deaths": stats["deaths"],
		"tier": unlocked_tier, "built": built, "stock": st, "free_beds": free_beds(),
		"decisions": stats["decisions"], "failed": stats["failed_decisions"]}
