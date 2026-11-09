class_name ColonyStory
extends RefCounted
## The colony's story, for people watching: a timeline of the moments that matter (births,
## deaths, buildings, meetings, new roles, seasons) and a short summary of each week. Weeks end
## on the morning after the weekly meeting, so each chapter includes what the meeting agreed.
## Read-only with respect to the sim: it only listens to events.

const MAX_TIMELINE := 500
const DAYS_PER_WEEK := 7

var sim: Sim
var timeline: Array = []      # {t, kind, text}  kind: birth death built expanded meeting role season tier couple
var weeks: Array = []         # {week, from_day, to_day, lines: [String]}, oldest first
var _w := {}                  # the week so far


func watch(p_sim: Sim) -> void:
	sim = p_sim
	var ev := sim.events
	ev.colonist_born.connect(func(c: Colonist) -> void:
		if not c.parent_ids.is_empty():
			_add("birth", "%s was born." % c.name)
			_w["births"] = int(_w.get("births", 0)) + 1)
	ev.colonist_died.connect(func(c: Colonist, cause: String) -> void:
		_add("death", "%s died (%s)." % [c.name, cause])
		var d: Dictionary = _w.get("deaths", {})
		d[cause] = int(d.get(cause, 0)) + 1
		_w["deaths"] = d)
	ev.couple_formed.connect(func(a: Colonist, b: Colonist) -> void:
		_add("couple", "%s and %s became a couple." % [a.name, b.name]))
	ev.building_completed.connect(func(b: Building) -> void:
		var label := String(b.def().get("label", b.type))
		_add("built", "The %s was finished." % label)
		var built: Dictionary = _w.get("built", {})
		built[label] = int(built.get(label, 0)) + 1
		_w["built"] = built)
	ev.building_expanded.connect(func(b: Building) -> void:
		var text := "The %s was enlarged: it seats %d now." % [b.def().get("label", b.type), b.seats()]
		_add("expanded", text)
		_w["expanded"] = (_w.get("expanded", []) as Array) + [text])
	ev.role_changed.connect(func(c: Colonist, role: String) -> void:
		if role != "":
			_add("role", "%s became the colony's %s." % [c.name, sim.role_name(role)])
			_w["roles"] = (_w.get("roles", []) as Array) + ["%s became the colony's %s" % [c.name, sim.role_name(role)]])
	ev.season_changed.connect(func(season: String) -> void:
		_add("season", "%s came." % season.capitalize())
		_w["seasons"] = (_w.get("seasons", []) as Array) + [season])
	ev.tier_unlocked.connect(func(tier: int) -> void:
		_add("tier", "The colony reached tier %d." % tier))
	ev.meeting_held.connect(func(plan: Dictionary) -> void:
		var crowd := (", %d stood outside" % int(plan["overflow"])) if int(plan["overflow"]) > 0 else ""
		var text := "Meeting: %d came%s; they agreed: %s." % [(plan["present"] as Array).size(), crowd, sim.meetings.plan_summary(plan)]
		_add("meeting", text)
		_w["meeting"] = text)
	ev.action_finished.connect(func(c: Colonist, a: ActionBase, ok: bool) -> void:
		if not ok or a == null:
			return
		var aid := String(a.def.get("id", ""))
		if not WORK_WORDS.has(aid):
			return
		var jobs: Dictionary = _w.get("jobs", {})
		var per: Dictionary = jobs.get(aid, {})
		per[c.name] = int(per.get(c.name, 0)) + 1
		jobs[aid] = per
		_w["jobs"] = jobs)
	ev.day_passed.connect(func(day: int) -> void:
		# A week ends the morning after its meeting day (day 8, 15, ...): it includes the meeting.
		if day > 1 and (day - 1) % DAYS_PER_WEEK == 0:
			close_week())
	_start_week()


const WORK_WORDS := {
	"gather_wood": "chopped wood", "gather_stone": "cut stone", "gather_food": "picked berries",
	"fetch_water": "fetched water", "farm": "worked the farms", "build": "built", "explore": "went exploring",
}


func _add(kind: String, text: String) -> void:
	timeline.append({"t": sim.clock, "kind": kind, "text": text})
	if timeline.size() > MAX_TIMELINE:
		timeline.pop_front()


func _start_week() -> void:
	_w = {"from_day": sim.day(), "stock": sim.stock.duplicate(), "pop": sim.alive_count()}


func week_number(from_day: int) -> int:
	return int(from_day / DAYS_PER_WEEK) + 1


func close_week() -> void:
	var lines := summary(_w)
	var week := {"week": week_number(int(_w["from_day"])), "from_day": int(_w["from_day"]), "to_day": sim.day() - 1, "lines": lines}
	weeks.append(week)
	sim.events.week_summarized.emit(week)
	_start_week()


## The week so far, as a few plain sentences.
func current() -> Array:
	return summary(_w)


func summary(w: Dictionary) -> Array:
	var L: Array = []
	var births := int(w.get("births", 0))
	var deaths: Dictionary = w.get("deaths", {})
	var died := 0
	var causes: PackedStringArray = []
	for k in deaths:
		died += int(deaths[k])
		causes.append("%d %s" % [int(deaths[k]), k])
	if births > 0 or died > 0:
		var parts: PackedStringArray = []
		if births > 0:
			parts.append("%d born" % births)
		if died > 0:
			parts.append("%d died (%s)" % [died, ", ".join(causes)])
		L.append("%s; %d people now." % [_cap(", ".join(parts)), sim.alive_count()])
	var built: Dictionary = w.get("built", {})
	if not built.is_empty():
		var bp: PackedStringArray = []
		for k in built:
			bp.append("%s %s" % [_count_word(int(built[k])), k + ("s" if int(built[k]) > 1 else "")])
		L.append("Built: %s." % _join_and(bp))
	for e in w.get("expanded", []):
		L.append(String(e))
	# Who did most of each kind of work (top worker per job, busiest jobs first).
	var jobs: Dictionary = w.get("jobs", {})
	var busiest: Array = []
	for aid in jobs:
		var per: Dictionary = jobs[aid]
		var top := ""
		var n := 0
		var total := 0
		for who in per:
			total += int(per[who])
			if int(per[who]) > n:
				n = int(per[who])
				top = who
		busiest.append({"aid": aid, "top": top, "n": n, "total": total})
	busiest.sort_custom(func(a, b) -> bool: return int(a["total"]) > int(b["total"]))
	var bz: PackedStringArray = []
	for b in busiest.slice(0, 3):
		bz.append("%s %s most (%d of %d times)" % [b["top"], WORK_WORDS[b["aid"]], b["n"], b["total"]])
	if not bz.is_empty():
		L.append("Busiest: %s." % "; ".join(bz))
	var roles: Array = w.get("roles", [])
	if not roles.is_empty():
		L.append("New roles: %s." % _join_and(PackedStringArray(roles)))
	var before: Dictionary = w.get("stock", {})
	var st: PackedStringArray = []
	for r in Knowledge.STOCK_KEYS:
		var a := int(before.get(r, 0))
		var b := int(sim.stock.get(r, 0))
		if absi(b - a) >= 10:
			st.append("%s %d → %d" % [r, a, b])
	if not st.is_empty():
		L.append("Stores: %s." % ", ".join(st))
	if w.has("meeting"):
		L.append(String(w["meeting"]))
	for s in w.get("seasons", []):
		L.append("%s came." % String(s).capitalize())
	if L.is_empty():
		L.append("A quiet week.")
	return L


func _cap(s: String) -> String:
	return s.substr(0, 1).to_upper() + s.substr(1)


func _count_word(n: int) -> String:
	return ["no", "a", "two", "three", "four", "five", "six"][n] if n < 7 else str(n)


func _join_and(parts: PackedStringArray) -> String:
	if parts.size() <= 1:
		return "".join(parts)
	return ", ".join(parts.slice(0, parts.size() - 1)) + " and " + parts[parts.size() - 1]


func to_dict() -> Dictionary:
	var w := _w.duplicate(true)
	return {"timeline": timeline.duplicate(true), "weeks": weeks.duplicate(true), "week": w}


func from_dict(d: Dictionary) -> void:
	timeline = d.get("timeline", []).duplicate(true)
	weeks = d.get("weeks", []).duplicate(true)
	_w = d.get("week", {}).duplicate(true)
	if _w.is_empty():
		_start_week()
