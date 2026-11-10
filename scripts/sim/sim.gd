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
## Minutes spent walking on each tile (y * size + x), fading slowly: the view wears paths into the
## grass where people actually go.
var footfall := PackedFloat32Array()
const FOOTFALL_FADE_PER_HOUR := 0.995
## Land anyone in the colony has seen (y * size + x: 1 = seen). Resources are only used where a
## colonist knows about them (Knowledge.places); the rest of the map has to be explored.
var map_options: Dictionary = {}   # map generation options (WorldMap.DEFAULT_OPTIONS); set before setup()
var discovered := PackedByteArray()
var discovered_count := 0
const LOOK := 6.0            # tiles a colonist sees around them
const FOUNDERS_KNOW := 10.0  # tiles around the camp the founders scouted before settling
var unlocked_tier: int = 0

var brains: Dictionary = {}             # brain id -> Brain
var default_brain_id: String = "utility"
## Optional teacher: a second brain (e.g. "utility") that also scores every decision another brain
## makes. Its answer is logged next to the real one (ctx["teacher"]), so training can relabel the
## states the learner actually reaches (learning from its own mistakes). It never acts.
var teacher_id: String = ""
var describer: StateDescriber = StateDescriber.new()
var action_types: Dictionary = {}       # type name -> Script

var decision_budget_usec := -1   # per-frame time for decisions (set by the game; -1 = no limit)
var decisions_spent_usec := 0
var stats := {"births": 0, "deaths": {}, "decisions": 0, "failed_decisions": 0, "requests_made": 0, "requests_followed": 0}

var weather: String = "clear"
var _last_season: String = ""
var _next_id: int = 1
var _next_request_id: int = 1
var _last_hour: int = -1
var _last_day: int = -1
var _used_names: Dictionary = {}


## ---- Setup ----------------------------------------------------------------

func setup(p_seed: int) -> void:
	content = Content.get_instance()
	meetings.watch(self)
	seed_value = p_seed
	rng.seed = p_seed
	_load_action_types()
	map.setup(int(map_options.get("size", content.cfg("map_size", 60))))
	map.generate(rng, self)
	# Keep the full options the map was made with (defaults included), so a save made now
	# rebuilds the same map even if the defaults change later.
	var full: Dictionary = WorldMap.DEFAULT_OPTIONS.duplicate()
	full.merge(map_options, true)
	map_options = full
	footfall.resize(map.size * map.size)
	discovered.resize(map.size * map.size)
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
	wildlife.setup(self)       # (after the start buildings, so no animal stands where they go)
	var ages: Array = content.cfg("founder_age_days", [5.0, 12.0])
	for i in int(map_options.get("settlers", start.get("colonists", 5))):
		var cell := map.nearest_walkable(map.center + Vector2i(rng.randi_range(-3, 3), rng.randi_range(-3, 3)))
		spawn_colonist(cell, rng.randf_range(float(ages[0]), float(ages[1])), [])
	for c: Colonist in colonists.values():
		observe_stock(c)   # the founders packed the stores themselves
		look_around(c, FOUNDERS_KNOW, Vector2(map.center))
	_update_tier(true)
	_assign_beds()
	_roll_weather()
	_last_season = season()
	_heat_houses()
	_rain_water()
	story.watch(self)


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
			var was := c.pos
			var st := c.action.tick(dt)
			if c.pos != was:
				_step_on(c.pos, dt)
			if st != ActionBase.RUNNING:
				_finish_action(c, st == ActionBase.DONE)
		if c.state == "idle" and clock >= c.hesitate_until:
			# In the game each frame has a little time for decisions; past it, the rest decide
			# next frame (a moment later). Headless runs have no limit.
			if decision_budget_usec < 0 or decisions_spent_usec < decision_budget_usec:
				var t0 := Time.get_ticks_usec()
				_request_decision(c)
				decisions_spent_usec += Time.get_ticks_usec() - t0
	for c in to_bury:
		colonists.erase(c.id)
		graveyard.append(c)
	_world_tick(dt)
	wildlife.update(dt)
	meetings.update()
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
		_roll_weather()
		wildlife.on_day()
		if season() != _last_season:
			_last_season = season()
			events.season_changed.emit(_last_season)
		events.day_passed.emit(day())
	events.ticked.emit(clock)


func _step_on(p: Vector2, dt: float) -> void:
	var x := roundi(p.x)
	var y := roundi(p.y)
	if x >= 0 and y >= 0 and x < map.size and y < map.size:
		footfall[y * map.size + x] += dt


## ---- Roles ------------------------------------------------------------------
## Nobody is assigned a job. People get better at what they do (skills), and someone whose best
## skill is well ahead of their others becomes known for it: "the colony's woodcutter".

func role_skills() -> Dictionary:
	return content.roles.get("skills", {})


var _role_cut: Dictionary = {}   # skill id -> skill level needed to be among the colony's best at it


func _roles_cfg(key: String, default: Variant) -> Variant:
	return content.roles.get(key, default)


## For each skill, the level you need to be in the top `top_share` of adults at it.
func _update_role_cuts() -> void:
	var adults: Array[Colonist] = []
	for c: Colonist in colonists.values():
		if c.is_adult():
			adults.append(c)
	var allowed := maxi(1, ceili(adults.size() * float(_roles_cfg("top_share", 0.15))))
	_role_cut.clear()
	for sk in role_skills():
		var levels: Array = adults.map(func(c: Colonist) -> float: return float(c.skills.get(sk, 0.0)))
		levels.sort()
		levels.reverse()
		_role_cut[sk] = float(levels[mini(allowed, levels.size()) - 1]) if not levels.is_empty() else INF


func _among_best(c: Colonist, sk: String) -> bool:
	return float(c.skills.get(sk, 0.0)) >= float(_role_cut.get(sk, INF)) and float(c.skills.get(sk, 0.0)) >= float(_roles_cfg("min_level", 1.5))


## The skill id `c` is known for right now, or "": what they've done most lately, if they're also
## among the colony's best at it. A role sticks until another job overtakes it clearly.
func role_for(c: Colonist) -> String:
	if not c.is_adult():
		return ""
	if _role_cut.is_empty():
		_update_role_cuts()
	var lead := float(_roles_cfg("lead", 1.3))
	var best := ""
	var bp := 0.0
	var second := 0.0
	for sk in role_skills():
		var p := float(c.practice.get(sk, 0.0))
		if p > bp:
			second = bp
			bp = p
			best = sk
		elif p > second:
			second = p
	var cur := c.role
	if cur != "" and _among_best(c, cur) and bp < lead * float(c.practice.get(cur, 0.0)):
		return cur          # nothing has clearly overtaken it
	if best != "" and _among_best(c, best) and bp >= lead * second:
		return best
	return ""


func role_name(skill_id: String) -> String:
	return String(role_skills().get(skill_id, {}).get("role", ""))


## The skill an action trains / benefits from ("" if none).
func skill_for_action(action_id: String) -> String:
	for sk in role_skills():
		if Array(role_skills()[sk].get("jobs", [])).has(action_id):
			return sk
	return ""


func _update_roles() -> void:
	var fade := pow(0.5, 1.0 / (24.0 * float(_roles_cfg("practice_half_life_days", 3.0))))
	for c: Colonist in colonists.values():
		for sk in c.practice:
			c.practice[sk] = float(c.practice[sk]) * fade
	_update_role_cuts()
	for c: Colonist in colonists.values():
		var r := role_for(c)
		if r == c.role:
			continue
		var was := c.role
		c.role = r
		c.knowledge.roles[c.id] = r
		if r != "":
			add_memory(c, "People have started calling me the colony's %s." % role_name(r), 3.5)
		elif was != "":
			var what := String(role_skills().get(was, {}).get("good_at", "that work"))
			add_memory(c, ("Others are better at %s than me now." if not _among_best(c, was) else "I haven't done much %s lately.") % what, 1.5)
		events.role_changed.emit(c, r)


## What `c` believes `o` is known for ("" if nothing / not heard).
func believed_role(c: Colonist, o: Colonist) -> String:
	if c == o:
		return c.role
	return String(c.knowledge.roles.get(o.id, ""))


## `to` learns what `from` is known for, and a couple of others `from` knows about.
func share_roles(from: Colonist, to: Colonist) -> void:
	to.knowledge.roles[from.id] = from.role
	var n := 0
	for oid in from.knowledge.roles:
		if n >= 2:
			break
		if oid != to.id and not to.knowledge.roles.has(oid) and colonists.has(oid):
			to.knowledge.roles[oid] = from.knowledge.roles[oid]
			n += 1


func _on_hour() -> void:
	_update_roles()
	meetings.on_hour()
	for i in footfall.size():
		footfall[i] *= FOOTFALL_FADE_PER_HOUR
	for n: ResNode in nodes.values():
		var regrow := float(n.def().get("regrow_per_hour", 0)) * float(season_def().get("regrow", 1.0))
		if regrow > 0.0:
			n.amount = minf(n.max_amount, n.amount + regrow)
	_assign_beds()
	_heat_houses()
	_rain_water()


func _world_tick(dt: float) -> void:
	for b: Building in buildings.values():
		if b.complete and b.farm_stage == "growing":
			var f: Dictionary = b.provides("farm", {})
			b.farm_growth += dt / (float(f.get("grow_hours", 36)) * 60.0) * crop_mult()
			if b.farm_growth >= 1.0:
				b.farm_growth = 1.0
				b.farm_stage = "ripe"
	for c: Colonist in colonists.values():
		if c.pregnant_until > 0.0 and clock >= c.pregnant_until:
			_give_birth(c)


## ---- Needs, aging, death ---------------------------------------------------

## ---- Seasons, weather, cold ---------------------------------------------------

func season() -> String:
	var order: Array = content.seasons.get("order", ["spring"])
	var per := int(content.seasons.get("days_per_season", 4))
	return String(order[(day() / maxi(per, 1)) % order.size()])


func season_def() -> Dictionary:
	return content.seasons.get("seasons", {}).get(season(), {})


func weather_def() -> Dictionary:
	return content.seasons.get("weather", {}).get(weather, {})


## Day within the season (0-based) and days left until `which` starts (0 = it's that season now).
func season_day() -> int:
	return day() % maxi(int(content.seasons.get("days_per_season", 4)), 1)


func days_until(which: String) -> int:
	var order: Array = content.seasons.get("order", [])
	var per := int(content.seasons.get("days_per_season", 4))
	var i := order.find(season())
	var j := order.find(which)
	if i < 0 or j < 0 or i == j:
		return 0
	return ((j - i + order.size()) % order.size()) * per - season_day()


## How cold it is right now (0 = mild, ~1+ = bitter): season + weather + night.
func cold() -> float:
	var k := float(season_def().get("cold", 0.0)) + float(weather_def().get("cold", 0.0))
	if is_night() and k > 0.0:
		k += float(content.seasons.get("night_cold", 0.25))
	return k


func crop_mult() -> float:
	return float(season_def().get("crop_growth", 1.0)) * float(weather_def().get("crop", 1.0))


func weather_work_mult() -> float:
	return float(weather_def().get("work", 1.0))


func _roll_weather() -> void:
	var odds: Dictionary = season_def().get("weather", {"clear": 1.0})
	var r := rng.randf()
	var acc := 0.0
	var pick := "clear"
	for w in odds:
		acc += float(odds[w])
		if r <= acc:
			pick = w
			break
	if pick != weather:
		weather = pick
		events.weather_changed.emit(weather)


## Warmth per hour a colonist gets right now: near a fire, or asleep in a heated home.
var _warm_spots: Array = []     # [[centre, reach, warmth]] of finished buildings that warm, this tick
var _warm_clock := -1.0


func warmth_gain(c: Colonist) -> float:
	if _warm_clock != clock:
		_warm_clock = clock
		_warm_spots.clear()
		for b: Building in buildings.values():
			if b.complete:
				var w := float(b.provides("warmth", 0.0))
				if w > 0.0:
					_warm_spots.append([b.center(), float(b.provides("warm_radius", 2.0)) + b.size.x * 0.5, w])
	var g := 0.0
	for spot in _warm_spots:
		if c.pos.distance_to(spot[0]) <= spot[1]:
			g = maxf(g, spot[2])
	if c.action != null and c.action.phase == "sleeping" and c.home_id >= 0:
		var home := buildings.get(c.home_id) as Building
		if home != null and home.heated:
			g = maxf(g, float(home.provides("heated_warmth", 0.0)))
	return g


## When it's cold, occupied houses burn firewood (wood from the shared stock) to stay heated.
func _heat_houses() -> void:
	var need_heat := cold() >= float(content.seasons.get("heating_from_cold", 0.3))
	var per := float(content.seasons.get("firewood_per_house_hour", 0.4))
	for b: Building in buildings.values():
		if not (b.complete and b.provides("heated_warmth") != null):
			continue
		var occupied := b.beds.any(func(x) -> bool: return int(x) >= 0)
		b.heated = false
		if need_heat and occupied and float(stock.get("wood", 0.0)) >= per:
			stock["wood"] = float(stock["wood"]) - per
			b.heated = true


## Rain and storms fill the stores a little (rain caught at the wells).
func _rain_water() -> void:
	var per := float(weather_def().get("rain_water_per_well_hour", 0.0))
	if per <= 0.0:
		return
	var wells := 0
	for b: Building in buildings.values():
		if b.complete and b.provides("water_source") != null:
			wells += 1
	if wells > 0:
		deposit("water", per * wells)


func _update_needs(c: Colonist, dt: float) -> void:
	var child_mult := 1.0 if c.is_adult() else float(content.cfg("child_need_decay_mult", 0.7))
	for need_id in content.needs:
		var nd: Dictionary = content.needs[need_id]
		var rate := float(nd.get("decay_per_hour", 1.0)) / 60.0 * dt * child_mult
		rate *= c.trait_mod(need_id + "_decay")
		if c.action != null:
			rate *= c.action.need_decay_mult(need_id)
		if nd.get("decay_scale", "") == "cold":
			# Drains only when it's cold; recovers slowly when mild; fires and heated homes warm.
			var k := cold()
			rate *= k
			if k <= 0.05:
				rate -= float(nd.get("recover_per_hour_when_mild", 0.0)) / 60.0 * dt
			rate -= warmth_gain(c) / 60.0 * dt
		var v := clampf(float(c.needs.get(need_id, 100.0)) - rate, 0.0, 100.0)
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
		look_around(c)   # a newborn knows only what's around them; the rest they hear in time
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
	for c: Colonist in colonists.values():
		look_around(c)
		_see_game(c)
	var stores: Array[Building] = []
	for b: Building in buildings.values():
		if b.complete and float(b.provides("storage", 0)) > 0.0:
			stores.append(b)
	for c: Colonist in colonists.values():
		for b in stores:
			if c.pos.distance_to(b.center()) <= SIGHT + b.size.x * 0.5:
				observe_stock(c)
				break


## What game animals `c` can see from where they stand: the nearest of each kind is remembered (and
## how many were there). Back at a spot where they'd seen some, and none there now: they've moved on.
func _see_game(c: Colonist) -> void:
	var seen_kind := {}
	for a: Dictionary in wildlife.animals.values():
		var dist: float = c.pos.distance_to(a["pos"])
		if dist > LOOK:
			continue
		var k := String(a["kind"])
		var e: Dictionary = seen_kind.get(k, {"d": INF, "cell": Vector2i.ZERO, "n": 0})
		e["n"] = int(e["n"]) + 1
		if dist < float(e["d"]):
			e["d"] = dist
			e["cell"] = Vector2i(a["pos"].round())
		seen_kind[k] = e
	for k in seen_kind:
		c.knowledge.see_game(k, seen_kind[k]["cell"], clock, int(seen_kind[k]["n"]))
	for k in c.knowledge.game.keys():
		if not seen_kind.has(k) and c.pos.distance_to(Vector2(c.knowledge.game[k]["cell"])) < 3.0:
			c.knowledge.forget_game(k)


## Passing on where game was seen: the listener takes it if it's fresher than what they know.
## Returns the kinds that were news to them.
func share_game(from: Colonist, to: Colonist) -> Array:
	var news := []
	var tr := trust(to, from)
	for e in from.knowledge.fresh_game(clock):
		if rng.randf() > 0.4 + 0.6 * tr:
			continue
		var had := to.knowledge.fresh_game(clock).any(func(x: Dictionary) -> bool: return x["kind"] == e["kind"])
		if to.knowledge.hear_game(e["kind"], e["cell"], clock - float(e["age_h"]) * 60.0, from.name, int(e["n"])) and not had:
			news.append({"kind": e["kind"], "cell": e["cell"]})
	return news


## ---- Exploration ----------------------------------------------------------

func is_discovered(cell: Vector2i) -> bool:
	return map.in_bounds(cell) and discovered[cell.y * map.size + cell.x] > 0


## `c` looks around (from `at`, default where they stand): they remember every resource within
## `radius`, forget ones they knew there that are gone, and the land becomes discovered.
## Returns how many places were new to them. Cheap to call often: it skips if they haven't moved.
func look_around(c: Colonist, radius: float = LOOK, at: Variant = null) -> int:
	var centre: Vector2 = c.pos if at == null else at
	var here := Vector2i(roundi(centre.x), roundi(centre.y))
	if at == null:
		if c.get_meta("looked_from", Vector2i(-99, -99)) == here:
			return 0
		c.set_meta("looked_from", here)
	var r := int(ceil(radius))
	var found := 0
	if c.knowledge.seen.size() != map.size * map.size:
		c.knowledge.seen.resize(map.size * map.size)
	for y in range(maxi(0, here.y - r), mini(map.size, here.y + r + 1)):
		for x in range(maxi(0, here.x - r), mini(map.size, here.x + r + 1)):
			var cell := Vector2i(x, y)
			if Vector2(cell - here).length() > radius:
				continue
			var i := y * map.size + x
			if discovered[i] == 0:
				discovered[i] = 1
				discovered_count += 1
			c.knowledge.seen[i] = 1
			var nid: Variant = map.node_at.get(cell, map.litter_at.get(cell))
			if nid != null and nodes.has(nid):
				var n: ResNode = nodes[nid]
				if not c.knowledge.knows_place(n.id):
					found += 1
				c.knowledge.see_place(n.id, n.type, n.cell, clock, n.amount)
	# Places they remember around here that aren't there any more.
	for nid in c.knowledge.places.keys():
		var p: Dictionary = c.knowledge.places[nid]
		if not nodes.has(nid) and Vector2(p["cell"] - here).length() <= radius:
			c.knowledge.forget_place(nid)
	return found


## Where to explore: a walkable spot about `distance` tiles away, in the direction with the most
## land this colonist hasn't seen (their own knowledge, not the colony's).
func exploration_target(c: Colonist, distance: int) -> Vector2i:
	var cands := []
	for k in 8:
		var dir := Vector2.from_angle(k * TAU / 8.0 + rng.randf_range(-0.2, 0.2))
		var aim := Vector2i((c.pos + dir * distance).round()).clamp(Vector2i.ZERO, Vector2i(map.size - 1, map.size - 1))
		var p := map.nearest_walkable(aim)
		if p.x < 0 or not map.in_bounds(p):
			continue
		var unknown := 0
		var total := 0
		for dy in range(-4, 5, 2):
			for dx in range(-4, 5, 2):
				var q := p + Vector2i(dx, dy)
				if map.in_bounds(q):
					total += 1
					if not _seen_by(c, q):
						unknown += 1
		cands.append({"cell": p, "score": float(unknown) / maxf(1.0, float(total)) + rng.randf() * 0.1})
	cands.sort_custom(func(a, b) -> bool: return a["score"] > b["score"])
	for cand in cands:
		if cand["score"] <= 0.15:
			break
		if cand["cell"] == c.cell or not map.find_path(c.cell, cand["cell"]).is_empty():
			return cand["cell"]
	return Vector2i(-1, -1)


## Up to `n` bits of bank `c` has seen, nearest first.
func seen_shores(c: Colonist, n: int) -> Array[Vector2i]:
	var cands: Array = []
	for s: Vector2i in map.shore_cells:
		var i := s.y * map.size + s.x
		if i < c.knowledge.seen.size() and c.knowledge.seen[i] > 0:
			cands.append([Vector2(s - c.cell).length_squared(), s])
	cands.sort_custom(func(a, b) -> bool: return a[0] < b[0])
	var out: Array[Vector2i] = []
	for e in cands.slice(0, n):
		out.append(e[1])
	return out


## The nearest bit of river bank `c` has seen, or (-1, -1).
func seen_shore(c: Colonist) -> Vector2i:
	var best := Vector2i(-1, -1)
	var best_d := INF
	for s: Vector2i in map.shore_cells:
		var i := s.y * map.size + s.x
		if i >= c.knowledge.seen.size() or c.knowledge.seen[i] == 0:
			continue
		var d := Vector2(s - c.cell).length_squared()
		if d < best_d:
			best_d = d
			best = s
	return best


## Has `c` seen this spot with their own eyes?
func _seen_by(c: Colonist, cell: Vector2i) -> bool:
	var i := cell.y * map.size + cell.x
	return i < c.knowledge.seen.size() and c.knowledge.seen[i] > 0


## After taking from a resource node, the colonist knows how much is left (or that it's gone).
func saw_node_after_use(c: Colonist, source: Dictionary) -> void:
	if source.get("kind") != "node":
		return
	var n := source["ref"] as ResNode
	if nodes.has(n.id):
		c.knowledge.see_place(n.id, n.type, n.cell, clock, n.amount)
	else:
		c.knowledge.forget_place(n.id)


func explored_share() -> float:
	return float(discovered_count) / float(maxi(1, discovered.size()))


## "north", "south-east"... from the camp, as people would say it (screen-up = north).
func direction_word(cell: Vector2i) -> String:
	var d := Vector2(cell - map.center)
	if d.length() < 4.0:
		return "near the camp"
	var names := ["east", "south-east", "south", "south-west", "west", "north-west", "north", "north-east"]
	var k := posmod(roundi(d.angle() / (TAU / 8.0)), 8)
	return "to the " + String(names[k])


const PLACES_PER_TALK := 3   # a conversation has room for a few "there's a good patch of X by Y"


## `from` mentions places `to` doesn't know, the ones that matter most first: kinds of resource
## the listener knows nowhere to get. Whether it sticks depends on how much they trust the
## speaker. Returns the resources that were news, e.g. [{resource: "stone", cell}].
func share_places(from: Colonist, to: Colonist, limit: int = PLACES_PER_TALK) -> Array:
	var news := {}
	var told := 0
	var known: Array = from.knowledge.places.keys().filter(func(nid) -> bool:
		var a := float(from.knowledge.places[nid].get("amt", -1.0))
		return not to.knowledge.knows_place(nid) and (a < 0.0 or a >= 1.0))
	if known.is_empty():
		return []
	# Which kinds of resource the listener has nowhere to get (worked out once, not per comparison:
	# meetings call this for every pair of people there).
	var lacks := {}
	for t in content.nodes:
		lacks[t] = 1 if to.knowledge.has_usable(t, clock, false) else 0
	var key := {}
	for nid in known:
		var pl: Dictionary = from.knowledge.places[nid]
		key[nid] = float(lacks.get(pl["type"], 1)) * 1e6 + Vector2(pl["cell"] - from.cell).length_squared()
	known.sort_custom(func(a, b) -> bool: return float(key[a]) < float(key[b]))
	var tr := trust(to, from)
	for nid in known:
		if told >= limit:
			break
		var p: Dictionary = from.knowledge.places[nid]
		told += 1
		if rng.randf() > 0.4 + 0.6 * tr:
			continue    # mentioned, but it didn't stick
		var had_type := to.knowledge.knows_type(String(p["type"]))
		if to.knowledge.hear_place(nid, p["type"], p["cell"], float(p["t"]), from.name, float(p.get("amt", -1.0))):
			if not had_type:
				news[String(content.nodes.get(p["type"], {}).get("resource", p["type"]))] = p["cell"]
	return news.keys().map(func(r) -> Dictionary: return {"resource": r, "cell": news[r]})


## How much `c` believes what `from` tells them (0..1): liking someone makes them more believable.
func trust(c: Colonist, from: Colonist) -> float:
	if from == null:
		return 0.5
	var t := 0.35 + float(c.affection.get(from.id, 0.0)) / 120.0
	if c.partner_id == from.id or c.parent_ids.has(from.id):
		t += 0.25
	return clampf(t, 0.1, 1.0)


## Someone asked `c` to do an action. Kept as a dated request (and a memory) that brains read.
func add_request(c: Colonist, from_name: String, action_id: String, trust_v: float, reconsider: bool = false) -> void:
	var def: Dictionary = content.actions_by_id.get(action_id, {})
	if def.is_empty():
		return
	c.requests = c.requests.filter(func(r) -> bool: return r["action"] != action_id and clock - float(r["t"]) < REQUEST_TTL)
	c.requests.append({"from": from_name, "action": action_id, "label": def.get("label", action_id), "t": clock, "trust": trust_v})
	stats["requests_made"] = int(stats.get("requests_made", 0)) + 1
	if c.requests.size() > 3:
		c.requests.pop_front()
	add_memory(c, "%s asked me to %s." % [from_name, def.get("label", action_id)], 3.0 + 4.0 * trust_v)
	# Being asked makes them stop and think again now (the brain still decides). Not while asleep,
	# not while already doing what was asked, and not while seeing to an urgent need (eating when
	# starving): they'd only pick it again, having lost the progress.
	if reconsider and c.state == "acting" and c.action != null and c.action.phase != "sleeping" \
			and String(c.action.def.get("id", "")) != action_id and not _urgent_need_action(c):
		_finish_action(c, false)
		c.hesitate_until = clock
	elif reconsider and c.state == "thinking":
		c.pending_request_id = -1   # the answer in flight was for a state without this request: ignore it
		c.state = "idle"
		c.hesitate_until = clock


const REQUEST_TTL := 24.0 * 60.0


## True if their current action is tending a need that is down in its two worst bands.
func _urgent_need_action(c: Colonist) -> bool:
	var need := String(c.action.params.get("need", ""))
	if need == "" or not content.needs.has(need):
		return false
	var th: Array = content.needs[need].get("thresholds", [10, 30])
	return float(c.needs.get(need, 100.0)) < float(th[1]) if th.size() > 1 else false


## Starting a job someone asked for fulfils that request (so it stops nagging them).
func _fulfil_requests(c: Colonist, action_id: String) -> void:
	for r in active_requests(c):
		if r["action"] == action_id:
			c.requests.erase(r)
			stats["requests_followed"] = int(stats.get("requests_followed", 0)) + 1
			add_memory(c, "I did what %s asked: %s." % [r["from"], r["label"]], 2.5)
			return
const GATHER_FOR := {"food": "gather_food", "water": "fetch_water", "wood": "gather_wood", "stone": "gather_stone"}
const SHORT_BELOW := {"food": 40.0, "water": 20.0, "wood": 25.0, "stone": 25.0}

## Optional: called with each conversation dict. Return true to take it over (e.g. the game asks
## the chat model to write the dialogue) and later call apply_conversation(conv, result).
## Unset (headless, training) = a quick symbolic exchange of facts happens immediately.
var conversation_handler: Callable
## Set by the game to have the chat model write the weekly meeting: (meeting) -> true if it will
## call meetings.apply() itself. Unset (headless): the meeting is worked out symbolically.
var meeting_handler: Callable
var meetings := ColonyMeetings.new()
var story := ColonyStory.new()          # timeline and weekly summaries, for watching
var wildlife := ColonyWildlife.new()    # rabbits and deer: grazing, fleeing, breeding, hunted
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
	# Talk also passes on where things are (whatever the words were): what's new gets remembered.
	var places_learned := []
	for pair in [[a, b], [b, a]]:
		var sp: Colonist = pair[0]
		var li: Colonist = pair[1]
		if not li.alive():
			continue
		for news in share_places(sp, li):
			places_learned.append({"who": li.name, "resource": news["resource"], "from": sp.name})
			add_memory(li, "%s told me where to find %s (%s)." % [sp.name, news["resource"], direction_word(news["cell"])], 3.0)
		for news in share_game(sp, li):
			var kd: Dictionary = wildlife.kind_def(news["kind"])
			add_memory(li, "%s saw %s %s." % [sp.name, kd.get("plural", news["kind"]), direction_word(news["cell"])], 2.0)
	conv["places_learned"] = places_learned
	# And what the last meeting decided, if one of them knows and the other doesn't.
	if not meetings.pass_on(a, b):
		meetings.pass_on(b, a)
	share_roles(a, b)
	share_roles(b, a)
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
	return nodes.get(map.node_at.get(cell, map.litter_at.get(cell, -1))) as ResNode


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
		var o := {"id": def["id"], "label": def["label"], "description": def["description"]}
		# Which follow-up question this choice needs (so Laya is only asked it when it matters).
		if def["type"] == "build":
			o["asks"] = "build_what"
		elif def["type"] in ["socialize", "inform"]:
			o["asks"] = "with_whom"
		opts.append(o)
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
			elif t.begins_with("expand:"):
				var d: Dictionary = content.buildings[t.substr(7)]
				var hall: Building = null
				for hb: Building in buildings.values():
					if hb.type == t.substr(7):
						hall = hb
				var nxt := hall.level_def(hall.level + 1) if hall != null else {}
				bopts.append({"id": t, "label": "make the %s bigger" % d.get("label", t),
					"description": "enlarge it from %d to %d seats" % [hall.seats() if hall != null else 0, int(nxt.get("seats", 0))]})
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
	if teacher_id != "" and teacher_id != brain.id and brains.has(teacher_id):
		(brains[teacher_id] as Brain).decide(ctx, self, func(d: Dictionary) -> void: ctx["teacher"] = d)
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
		_fulfil_requests(c, String(decision.get("action", "")))
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

## Places to get `resource`. With a `knower`, only resource nodes they know about count (they
## can't use a tree nobody has seen).
func find_sources(specs: Array, resource: String, from: Vector2i, amount: float, for_gather: bool = false, knower: Colonist = null) -> Array:
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
			var pool: Array = nodes.values()
			if knower != null:
				pool = []
				for p in knower.knowledge.places_of(t, from):
					if nodes.has(p["id"]):
						pool.append(nodes[p["id"]])
					else:
						knower.knowledge.forget_place(p["id"])   # they'd go and find it gone
			for n: ResNode in pool:
				if knower != null and clock < n.unreachable_until:
					knower.knowledge.block_place(n.id, n.unreachable_until)   # they'd find they can't get there
				var need := String(n.def().get("needs_tool", ""))
				if knower != null and need != "" and not knower.has_tool(need):
					continue     # can't fell a tree with bare hands
				if n.type == t and n.amount >= 1.0 and (not for_gather or n.workers < 2) and clock >= n.unreachable_until:
					cands.append({"kind": "node", "ref": n, "cells": [n.cell], "d": Vector2(n.cell - from).length_squared()})
		elif spec.begins_with("building:"):
			var t := spec.substr(9)
			for b: Building in buildings.values():
				if b.complete and b.type == t:
					cands.append({"kind": "building", "ref": b, "cells": b.cells(), "d": Vector2(b.cell - from).length_squared()})
		elif spec == "tile:water":
			for s in map.shore_cells:
				cands.append({"kind": "tile", "ref": s, "cells": [s], "d": Vector2(s - from).length_squared()})
	# Nearest first (a native sort on distance, ties in list order: much faster than sort_custom).
	assert(cands.size() < 8192)
	var keys := PackedFloat64Array()
	keys.resize(cands.size())
	for i in cands.size():
		keys[i] = float(cands[i]["d"]) * 8192.0 + i
	keys.sort()
	# The nearest few that have somewhere to stand next to them. Walled-in ones (no free side) are
	# skipped, and a colonist who knows of one learns it's no use for now.
	var out := []
	for key in keys:
		var cand: Dictionary = cands[int(key) % 8192]
		if out.size() >= 4:
			break
		var typed: Array[Vector2i] = []
		typed.assign(cand["cells"])
		var acc := map.access_cell(typed, from)
		if acc.x >= 0:
			cand["access"] = acc
			out.append(cand)
		elif knower != null and cand["kind"] == "node":
			knower.knowledge.block_place((cand["ref"] as ResNode).id, clock + 12.0 * 60.0)
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
			if n.amount < 1.0:      # less than a trip's worth: it's used up
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
		if n.amount < 1.0:          # less than a trip's worth: it's used up
			_deplete(n)
		return got
	return amount


## Sticks and pebbles where something is being built are cleared away.
func _clear_litter(cells: Array[Vector2i]) -> void:
	for cell in cells:
		if map.litter_at.has(cell):
			var lit := nodes.get(map.litter_at[cell]) as ResNode
			map.litter_at.erase(cell)
			if lit != null:
				nodes.erase(lit.id)
				events.node_depleted.emit(lit)


## ---- Tools ------------------------------------------------------------------

func give_tool(c: Colonist, tool: String) -> void:
	var d: Dictionary = content.tools.get(tool, {})
	c.tools[tool] = float(d.get("uses", 30)) * colony_mult("tool_uses")
	add_memory(c, "I made a %s." % d.get("label", tool), 2.5)
	events.tool_made.emit(c, tool)


## One use of the tool a job needs; it breaks when the uses run out.
func use_tool(c: Colonist, tool: String) -> void:
	if tool == "" or not c.tools.has(tool):
		return
	c.tools[tool] = float(c.tools[tool]) - 1.0
	if float(c.tools[tool]) <= 0.0:
		c.tools.erase(tool)
		add_memory(c, "My %s broke." % content.tools.get(tool, {}).get("label", tool), 2.0)


## The tool needed to work a source ("" if bare hands will do).
func tool_for(source: Dictionary) -> String:
	return String(source_def(source).get("needs_tool", ""))


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
	else:
		map.litter_at[cell] = n.id
	return n


func _deplete(n: ResNode) -> void:
	if float(n.def().get("regrow_per_hour", 0)) > 0.0:
		return  # bushes stay and regrow
	nodes.erase(n.id)
	if map.litter_at.get(n.cell, -1) == n.id:
		map.litter_at.erase(n.cell)
	else:
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
		if b.is_site():
			var d := Vector2(b.cell - from).length_squared()
			if d < best_d:
				best_d = d
				best = b
	return best


## The nearest finished building that provides `key` (or, with colony_effect, has `key` among
## its colony-wide effects, e.g. the workshop's tool_uses).
func nearest_building_with(key: String, from: Vector2i, colony_effect: bool = false) -> Building:
	var best: Building = null
	var best_d := INF
	for b: Building in buildings.values():
		var has: bool = (b.provides("colony", {}) as Dictionary).has(key) if colony_effect else b.provides(key) != null
		if b.complete and has:
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
	for b: Building in buildings.values():
		if can_expand(b):
			out.append("expand:" + b.type)
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


## The biggest footprint a building type can grow to (its last level), or its footprint.
func max_footprint_of(type: String) -> Vector2i:
	var levels: Array = content.buildings.get(type, {}).get("levels", [])
	var fp := footprint_of(type)
	for lv in levels:
		var f: Array = lv.get("footprint", [fp.x, fp.y])
		fp = Vector2i(maxi(fp.x, int(f[0])), maxi(fp.y, int(f[1])))
	return fp


## How much land to keep for a building that grows: its biggest size if that fits near the
## village, else the biggest size that does (a hall far from everyone is no use).
const NEAR_VILLAGE := 14.0


func room_for(type: String) -> Vector2i:
	# (Same map, same buildings: same answer. A hall placement asks this three times.)
	var key := "%d|%d" % [map.version, buildings.size()]
	if key != _room_cache_key:
		_room_cache.clear()
		_room_cache_key = key
	if not _room_cache.has(type):
		_room_cache[type] = _room_for(type)
	return _room_cache[type]


func _room_for(type: String) -> Vector2i:
	var near := float(content.buildings.get(type, {}).get("near_village", NEAR_VILLAGE))
	var base := footprint_of(type)
	var levels: Array = content.buildings.get(type, {}).get("levels", [])
	var sizes: Array[Vector2i] = []
	for lv in levels:
		var f: Array = lv.get("footprint", [base.x, base.y])
		sizes.append(Vector2i(int(f[0]), int(f[1])))
	sizes.reverse()
	for sz in sizes:
		for origin in _spots(sz, true):
			if Vector2(origin + sz / 2 - map.center).length() <= near:
				return sz
			break
	return base


func find_build_spot(type: String, from: Vector2i) -> Vector2i:
	## First free spot (spiralling out from the centre) that `from` can actually walk to. For
	## buildings that grow, there must be room for the size they'll grow to (room_for).
	var size := room_for(type) if content.buildings.get(type, {}).has("levels") else footprint_of(type)
	# Candidates in the same spiral order as _spots, checked as they're found (stops at the first
	# reachable one instead of listing them all first). Gives up after 31, as before.
	var checked := 0
	var found := Vector2i(-1, -1)
	var c := map.center
	var lands := _land_rects()
	var grid := fit_grid()
	for r in range(0, map.size / 2):
		for dy in range(-r, r + 1):
			for dx in range(-r, r + 1):
				if maxi(absi(dx), absi(dy)) != r:
					continue
				var origin := c + Vector2i(dx, dy)
				if not _fits(origin, size, lands, grid):
					continue
				var cells: Array[Vector2i] = []
				for y in size.y:
					for x in size.x:
						cells.append(origin + Vector2i(x, y))
				if reach_cell(from, cells).x >= 0:
					return origin
				checked += 1
				if checked > 30:
					return found
	return found


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
		if b.is_site():
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
	var room := room_for(type) if d.has("levels") else size
	if spot.x < 0:
		spot = _find_spot(room)
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
	if room != size:
		b.grow_rect = Rect2i(spot, room)    # kept free so it can be enlarged later
	b.work_needed = float(d.get("work", 60))
	for i in int(d.get("provides", {}).get("beds", 0)):
		b.beds.append(-1)
	b.placed_at = clock
	b.placed_by = by.name if by != null else ""
	buildings[b.id] = b
	_clear_litter(b.cells())
	if not d.get("walkable", false):
		for cell in b.cells():
			map.building_at[cell] = b.id
			map.refresh_cell(cell)
	if by != null:
		add_memory(by, "I started building a %s." % d.get("label", type), 3.0)
	events.building_placed.emit(b)
	return b


## ---- Enlarging buildings (the meeting hall) ------------------------------------

## Can `b` be enlarged now: it has a next level, isn't already being enlarged, the colony can pay
## for it, and there's room around it for the bigger footprint.
func can_expand(b: Building) -> bool:
	if not b.complete or b.expanding:
		return false
	var nxt := b.level_def(b.level + 1)
	if nxt.is_empty() or not affordable(nxt.get("cost", {})):
		return false
	for o: Building in buildings.values():
		if o.replaces_id == b.id:
			return false      # a bigger one is already going up elsewhere
	return _expansion_origin(b).x >= 0 or _relocation_spot(b).x >= 0


## No room to enlarge it where it stands: a free spot near the village for the next size.
func _relocation_spot(b: Building) -> Vector2i:
	var fp: Array = b.level_def(b.level + 1).get("footprint", [b.size.x, b.size.y])
	var size := Vector2i(int(fp[0]), int(fp[1]))
	var near := float(b.def().get("near_village", NEAR_VILLAGE)) * 1.3
	for origin in _spots(size, true):
		if Vector2(origin + size / 2 - map.center).length() <= near:
			return origin
		break
	return Vector2i(-1, -1)


## Where the bigger footprint can go: it has to contain the current one.
func _expansion_origin(b: Building) -> Vector2i:
	var fp: Array = b.level_def(b.level + 1).get("footprint", [b.size.x, b.size.y])
	var size := Vector2i(int(fp[0]), int(fp[1]))
	var grow := size - b.size
	var best := Vector2i(-1, -1)
	var best_d := INF
	for oy in range(grow.y + 1):
		for ox in range(grow.x + 1):
			var origin := b.cell - Vector2i(ox, oy)
			if _fits_except(origin, size, b):
				var d := Vector2(origin + size / 2 - map.center).length()
				if b.grow_rect.size != Vector2i.ZERO and b.grow_rect.encloses(Rect2i(origin, size)):
					d -= 1000.0     # the land it kept for this
				if d < best_d:
					best_d = d
					best = origin
	return best


## Like _fits, but the building's own cells don't count as in the way.
func _fits_except(origin: Vector2i, size: Vector2i, b: Building) -> bool:
	for y in range(-1, size.y + 1):
		for x in range(-1, size.x + 1):
			var cell := origin + Vector2i(x, y)
			if not map.in_bounds(cell):
				return false
			if (map.building_at.has(cell) and map.building_at[cell] != b.id) or map.node_at.has(cell):
				return false
			var inside := x >= 0 and y >= 0 and x < size.x and y < size.y
			if inside and map.tile(cell) != WorldMap.Tile.GRASS and not map.building_at.has(cell):
				return false
			if inside and map.height_at(cell) != map.height_at(b.cell):
				return false
	for o: Building in buildings.values():
		var land := o.land()
		if o != b and Rect2i(land.position - Vector2i.ONE, land.size + Vector2i(2, 2)).intersects(Rect2i(origin, size)):
			return false
	return true


## Start enlarging the building of this type: pay, mark out the bigger footprint, and it becomes a
## site builders can work on (it stays usable meanwhile). Returns it, or null.
func start_expansion(type: String, by: Colonist) -> Building:
	for b: Building in buildings.values():
		if b.type != type or not can_expand(b):
			continue
		var nxt := b.level_def(b.level + 1)
		var origin := _expansion_origin(b)
		if origin.x < 0:
			return _relocate(b, by)
		var fp: Array = nxt.get("footprint", [b.size.x, b.size.y])
		for r in nxt.get("cost", {}):
			stock[r] = float(stock[r]) - float(nxt["cost"][r])
		events.stock_changed.emit(stock)
		b.prev_cell = b.cell
		b.prev_size = b.size
		b.cell = origin
		b.size = Vector2i(int(fp[0]), int(fp[1]))
		b.expanding = true
		b.work_done = 0.0
		b.work_needed = float(nxt.get("work", 600))
		_clear_litter(b.cells())
		for cell in b.cells():
			map.building_at[cell] = b.id
			map.refresh_cell(cell)
		if by != null:
			add_memory(by, "I started making the %s bigger." % b.def().get("label", type), 3.0)
		events.building_expansion_started.emit(b)
		return b
	return null


## Build the next size on a new spot; the old one stays in use until it's finished.
func _relocate(old: Building, by: Colonist) -> Building:
	var nxt := old.level_def(old.level + 1)
	var spot := _relocation_spot(old)
	if spot.x < 0:
		return null
	for r in nxt.get("cost", {}):
		stock[r] = float(stock[r]) - float(nxt["cost"][r])
	events.stock_changed.emit(stock)
	var fp: Array = nxt.get("footprint", [old.size.x, old.size.y])
	var b := Building.new()
	b.id = _new_id()
	b.type = old.type
	b.cell = spot
	b.size = Vector2i(int(fp[0]), int(fp[1]))
	b.level = old.level + 1
	b.work_needed = float(nxt.get("work", 600))
	b.replaces_id = old.id
	b.placed_at = clock
	b.placed_by = by.name if by != null else ""
	buildings[b.id] = b
	_clear_litter(b.cells())
	for cell in b.cells():
		map.building_at[cell] = b.id
		map.refresh_cell(cell)
	if by != null:
		add_memory(by, "There was no room to make the %s bigger, so I started a bigger one nearby." % b.def().get("label", b.type), 3.0)
	events.building_placed.emit(b)
	events.building_expansion_started.emit(b)
	return b


## Take a building down (the old hall once its bigger replacement is finished).
func remove_building(b: Building) -> void:
	_warm_clock = -1.0
	buildings.erase(b.id)
	for cell in b.cells():
		if map.building_at.get(cell, -1) == b.id:
			map.building_at.erase(cell)
			map.refresh_cell(cell)
	events.building_removed.emit(b)


func _finish_expansion(b: Building, by: Colonist) -> void:
	b.expanding = false
	b.level += 1
	b.work_done = b.work_needed
	var label: String = b.def().get("label", b.type)
	for c: Colonist in colonists.values():
		add_memory(c, ("I finished making the %s bigger: it seats %d now." if c == by else "The %s is bigger now: it seats %d.") % [label, b.seats()], 3.0)
	events.building_expanded.emit(b)


func complete_building(b: Building, by: Colonist, quiet: bool = false) -> void:
	_warm_clock = -1.0     # (a new fire or house: the warm spots list is rebuilt)
	if b.expanding:
		_finish_expansion(b, by)
		return
	if b.replaces_id >= 0:
		# The bigger hall built elsewhere is done: everyone moves over and the old one comes down.
		var old := buildings.get(b.replaces_id) as Building
		b.replaces_id = -1
		b.complete = true
		b.work_done = b.work_needed
		b.completed_at = clock
		if old != null:
			remove_building(old)
		for c: Colonist in colonists.values():
			add_memory(c, "The new %s is finished: it seats %d." % [b.def().get("label", b.type), b.seats()], 3.0)
		events.building_expanded.emit(b)
		return
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
	for origin in _spots(size, true):
		return origin
	return Vector2i(-1, -1)


func _spots(size: Vector2i, first_only: bool = false) -> Array[Vector2i]:
	## Every valid origin, spiralling out from the colony centre (nearest first); with first_only,
	## just the nearest (same answer, without scanning the rest).
	var out: Array[Vector2i] = []
	var c := map.center
	var lands := _land_rects()
	var grid := fit_grid()
	for r in range(0, map.size / 2):
		for dy in range(-r, r + 1):
			for dx in range(-r, r + 1):
				if maxi(absi(dx), absi(dy)) != r:
					continue
				var origin := c + Vector2i(dx, dy)
				if _fits(origin, size, lands, grid):
					out.append(origin)
					if first_only:
						return out
		if out.size() >= 40:
			break
	return out


var _fit_grid: FitGrid
var _fit_grid_version := -1
var _room_cache := {}         # type -> size, valid for _room_cache_key
var _room_cache_key := ""


## The fast fit check for the current map (rebuilt only when a cell has changed).
func fit_grid() -> FitGrid:
	if _fit_grid == null or _fit_grid_version != map.version:
		_fit_grid = FitGrid.new(map)
		_fit_grid_version = map.version
	return _fit_grid


## Every building's land with a 1-tile margin (what a new footprint mustn't overlap).
func _land_rects() -> Array[Rect2i]:
	var out: Array[Rect2i] = []
	for b: Building in buildings.values():
		var land := b.land()
		out.append(Rect2i(land.position - Vector2i.ONE, land.size + Vector2i(2, 2)))
	return out


func _fits(origin: Vector2i, size: Vector2i, lands: Variant = null, grid: FitGrid = null) -> bool:
	## Clear footprint plus a 1-tile walkway ring (no buildings, trees or rocks in the ring),
	## so buildings never wall each other off.
	if grid != null:
		if not grid.fits(origin, size):
			return false
		return _clear_of_lands(origin, size, lands)
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
			if inside and map.height_at(cell) != map.height_at(origin):
				return false     # buildings need flat ground
	return _clear_of_lands(origin, size, lands)


## Walkable farm plots also must not overlap other walkable sites, nor land kept to grow into.
func _clear_of_lands(origin: Vector2i, size: Vector2i, lands: Variant) -> bool:
	var foot := Rect2i(origin, size)
	for land: Rect2i in (lands if lands != null else _land_rects()):
		if land.intersects(foot):
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
