class_name StateDescriber
extends RefCounted
## Turns a colonist's situation into short plain text for Laya (and the decision log).
## Laya's English checkpoint reads ~320 tokens of state, so keep this compact.
## Bump VERSION whenever the format changes so logged data stays comparable.

const VERSION := "v8"   # v8: + game animals seen (hunting). v7: + tools. v6: + weekly meeting and the colony plan. v5: + roles / what they're good at. v4: + where they know resources are. v3: + season, weather, warmth, heating. v2: beliefs (seen/heard) and requests
const MAX_MEMORIES := 3


## "Has a stone hatchet (well used) and no pickaxe (needed for big rocks)."
func tools_phrase(sim: Sim, c: Colonist) -> String:
	var have: PackedStringArray = []
	var lack: PackedStringArray = []
	for t in sim.content.tools:
		var d: Dictionary = sim.content.tools[t]
		if c.has_tool(t):
			var left := float(c.tools[t]) / maxf(1.0, float(d.get("uses", 30)) * sim.colony_mult("tool_uses"))
			have.append("a %s%s" % [d.get("label", t), " (nearly worn out)" if left < 0.25 else (" (well used)" if left < 0.6 else "")])
		elif _needed_for(sim, t) != "":   # (no rod is nothing to remark on)
			lack.append("no %s" % String(d.get("label", t)).replace("stone ", ""))
	var out := "%s has %s" % [c.name, " and ".join(have) if not have.is_empty() else "no tools"]
	if not lack.is_empty() and not have.is_empty():
		out += ", " + " and ".join(lack)
	var needs: PackedStringArray = []
	for t in sim.content.tools:
		var what := _needed_for(sim, t)
		if what != "":
			needs.append("%s need a %s" % [what, String(sim.content.tools[t].get("label", t)).replace("stone ", "")])
	return out + (" (%s)." % ", ".join(needs) if not needs.is_empty() else ".")


## Which resource nodes need this tool, said as people would ("trees", "rocks"), or "".
func _needed_for(sim: Sim, tool: String) -> String:
	var kinds: PackedStringArray = []
	for t in sim.content.nodes:
		if String(sim.content.nodes[t].get("needs_tool", "")) == tool:
			kinds.append(String(t).replace("_", " ") + "s")
	return " and ".join(kinds)


## The weekly meeting and the plan, e.g. "The weekly meeting is in 3 hours at the meeting hall.
## Colony plan (agreed at the meeting on day 7): chop wood (Zed, Pia), build a house."
func meeting_phrase(sim: Sim, c: Colonist) -> String:
	var out: PackedStringArray = []
	var m := sim.meetings
	if not m.current.is_empty() and m.open_for(c):
		out.append("The meeting bell has rung: everyone is gathering at the %s." % (m.current["place"] as Building).def().get("label", "campfire"))
	elif m.hours_until_next() > 0.0 and c.is_adult():
		var place := m.meeting_place()
		out.append("The weekly meeting is in %d hours at the %s." % [ceili(m.hours_until_next()), place.def().get("label", "campfire") if place != null else "campfire"])
	if m.knows_plan(c):
		out.append("Colony plan (agreed at the meeting on day %d): %s." % [int(m.plan["day"]), m.plan_summary()])
	return " ".join(out)


## What they're good at and who they know is good at what, e.g. "Zed is the colony's woodcutter
## (best at chopping wood, also building). Known for their work: Pia (farmer), Theo (builder)."
func work_phrase(sim: Sim, c: Colonist) -> String:
	var rs := sim.role_skills()
	var mine: Array = []
	for sk in rs:
		if float(c.skills.get(sk, 0.0)) >= float(sim.content.roles.get("getting_good_at", 0.5)):
			mine.append(sk)
	mine.sort_custom(func(a, b) -> bool: return float(c.skills[a]) > float(c.skills[b]))
	var out := ""
	if c.role != "":
		out = "%s is the colony's %s" % [c.name, sim.role_name(c.role)]
		var also := mine.filter(func(sk) -> bool: return sk != c.role).slice(0, 1)
		out += " (also handy at %s)." % rs[also[0]]["good_at"] if not also.is_empty() else "."
	elif not mine.is_empty():
		out = "%s is getting good at %s." % [c.name, " and ".join(mine.slice(0, 2).map(func(sk) -> String: return String(rs[sk]["good_at"])))]
	# Who they've heard (or seen at the meeting) is known for their work.
	var known: PackedStringArray = []
	for oid in c.knowledge.roles:
		var o := sim.colonists.get(oid) as Colonist
		var r := String(c.knowledge.roles[oid])
		if o != null and o != c and r != "":
			known.append("%s (%s)" % [o.name, sim.role_name(r)])
		if known.size() >= 4:
			break
	if not known.is_empty():
		out += (" " if out != "" else "") + "Known for their work: %s." % ", ".join(known)
	return out


## What they know about where resources are, e.g. "Knows where to find: wood (5 places, nearest
## 4 tiles away), food (2 places, nearest 9 tiles away), stone (nowhere they know of)."
func places_phrase(sim: Sim, c: Colonist) -> String:
	# Only what they remember: places they've seen or heard of that had something left.
	var by_res := {}
	var spent := {}
	for t in sim.content.nodes:
		var d: Dictionary = sim.content.nodes[t]
		var res := String(d.get("resource", t))
		var u := c.knowledge.usable_summary(t, c.cell, sim.clock, float(d.get("regrow_per_hour", 0)) > 0.0)
		if int(u["usable"]) < int(u["known"]):
			spent[res] = true
		if int(u["usable"]) > 0:
			var e: Dictionary = by_res.get(res, {"n": 0, "d": INF})
			e["n"] = int(e["n"]) + int(u["usable"])
			e["d"] = minf(float(e["d"]), float(u["nearest"]))
			by_res[res] = e
	var out: PackedStringArray = []
	for t in sim.content.nodes:
		var res := String(sim.content.nodes[t].get("resource", t))
		if out.size() > 0 and Array(out).any(func(x: String) -> bool: return x.begins_with(res + " ")):
			continue
		if by_res.has(res):
			var e: Dictionary = by_res[res]
			out.append("%s (%d place%s, nearest %d tiles away)" % [res, e["n"], "" if e["n"] == 1 else "s", int(e["d"])])
		elif spent.has(res):
			out.append("%s (only places they saw picked clean)" % res)
		else:
			out.append("%s (nowhere they know of)" % res)
	return "Knows where to find: %s." % ", ".join(out)


## Where they last saw (or heard of) game, e.g. "Game: deer seen 7 tiles away 2h ago; rabbits
## (Pia said) 12 tiles away 5h ago." or "No game seen lately."
func game_phrase(sim: Sim, c: Colonist) -> String:
	var out: PackedStringArray = []
	for e in c.knowledge.fresh_game(sim.clock):
		var label := String(sim.wildlife.kind_def(e["kind"]).get("plural", e["kind"]))
		var when := "just now" if float(e["age_h"]) < 1.0 else "%dh ago" % int(e["age_h"])
		var how := "seen" if e["src"] == "seen" else "(%s said)" % e["from"]
		out.append("%s %s %d tiles away %s" % [label, how, int(Vector2(e["cell"] - c.cell).length()), when])
	return "Game: %s." % "; ".join(out) if not out.is_empty() else "No game seen lately."


func describe(sim: Sim, c: Colonist) -> String:
	var parts: PackedStringArray = []
	var trait_texts: PackedStringArray = []
	for t in c.traits:
		trait_texts.append(str(sim.content.traits.get(t, {}).get("text", t)))
	parts.append("%s is a%s %s colonist, %s." % [c.name, "n" if c.is_adult() else "", c.life_stage(), ", ".join(trait_texts)])
	var work := work_phrase(sim, c)
	if work != "":
		parts.append(work)
	if c.is_adult():
		parts.append(tools_phrase(sim, c))
	parts.append("It is day %d, %s, %s%s." % [sim.day(), _part_of_day(sim), _season_phrase(sim),
		(", " + String(sim.weather_def().get("label", sim.weather))) if sim.weather != "clear" else ""])
	if sim.cold() >= float(sim.content.seasons.get("heating_from_cold", 0.3)):
		var home := sim.buildings.get(c.home_id) as Building
		if home != null:
			parts.append("Home is heated." if home.heated else "Home is cold (no firewood).")

	var need_words: PackedStringArray = []
	for need_id in sim.content.needs:
		need_words.append(sim.need_word(c, need_id))
	parts.append("%s is %s." % [c.name, _join_and(need_words)])

	# Not the true stock: what this colonist believes, and how they know it.
	var st: PackedStringArray = []
	for k in Knowledge.STOCK_KEYS:
		st.append(c.knowledge.phrase(k, sim.clock))
	parts.append("%s thinks the stores hold: %s." % [c.name, ", ".join(st)])
	parts.append(places_phrase(sim, c))
	if c.is_adult():
		parts.append(game_phrase(sim, c))

	var built := {}
	var sites: PackedStringArray = []
	for b: Building in sim.buildings.values():
		var lbl: String = b.def().get("label", b.type)
		if b.complete:
			built[lbl] = int(built.get(lbl, 0)) + 1
		if b.expanding:
			sites.append("%s (being enlarged) %d%%" % [lbl, int(b.progress() * 100)])
		elif not b.complete:
			sites.append("%s %d%%" % [lbl, int(b.progress() * 100)])
	var built_txt: PackedStringArray = []
	for k in built:
		built_txt.append(k if built[k] == 1 else "%d %ss" % [built[k], k])
	parts.append("Built: %s." % (", ".join(built_txt) if not built_txt.is_empty() else "nothing"))
	if not sites.is_empty():
		parts.append("Unfinished: %s." % ", ".join(sites))
	var ripe := 0
	for b: Building in sim.buildings.values():
		if b.complete and b.farm_stage == "ripe":
			ripe += 1
	if ripe > 0:
		parts.append("%d farm%s ready to harvest." % [ripe, "s" if ripe > 1 else ""])
	parts.append("Population %d, free beds %d." % [sim.alive_count(), maxi(0, sim.free_beds())])

	if c.partner_id >= 0 and sim.colonists.has(c.partner_id):
		parts.append("Partner: %s." % (sim.colonists[c.partner_id] as Colonist).name)
	parts.append("Has a home bed." if c.home_id >= 0 else "Has no bed.")
	if c.pregnant_until > 0.0:
		parts.append("Expecting a child.")

	var meet := meeting_phrase(sim, c)
	if meet != "":
		parts.append(meet)

	var reqs := sim.active_requests(c)
	if not reqs.is_empty():
		var rq: PackedStringArray = []
		for r in reqs.slice(maxi(0, reqs.size() - 2)):
			rq.append("%s asked %s to %s (%dh ago)" % [r["from"], c.name, r["label"], int((sim.clock - float(r["t"])) / 60.0)])
		parts.append("Requests: %s." % "; ".join(rq))

	var mems := c.top_memories(sim.clock, MAX_MEMORIES)
	if not mems.is_empty():
		var mt: PackedStringArray = []
		for m in mems:
			mt.append(str(m["text"]))
		parts.append("Memories: %s" % " ".join(mt))
	return " ".join(parts)


func relation_text(sim: Sim, c: Colonist, o: Colonist) -> String:
	var rel := ""
	if c.partner_id == o.id:
		rel = "your partner"
	elif c.parent_ids.has(o.id):
		rel = "your parent"
	elif o.parent_ids.has(c.id):
		rel = "your child"
	else:
		var a := float(c.affection.get(o.id, 0.0))
		rel = "a close friend" if a >= 60 else "a friend" if a >= 30 else "an acquaintance" if a >= 10 else "someone you dislike" if a <= -20 else "someone you barely know"
	var dist := c.pos.distance_to(o.pos)
	var role := sim.believed_role(c, o)
	return "%s, %s%s" % [rel, "nearby" if dist < 8.0 else "far away", (", the colony's " + sim.role_name(role)) if role != "" else ""]


func _season_phrase(sim: Sim) -> String:
	var per := int(sim.content.seasons.get("days_per_season", 4))
	var d := sim.season_day()
	var when := "early" if d < per / 3.0 else ("late" if d >= per * 2 / 3.0 else "mid")
	var p := "%s %s" % [when, sim.season()]
	if sim.season() != "winter" and sim.days_until("winter") <= per:
		p += " (winter in %d day%s)" % [sim.days_until("winter"), "" if sim.days_until("winter") == 1 else "s"]
	return p


func _part_of_day(sim: Sim) -> String:
	var h := sim.time_of_day()
	if sim.is_night():
		return "night"
	if h < 11.0:
		return "morning"
	if h < 17.0:
		return "afternoon"
	return "evening"


func _join_and(words: PackedStringArray) -> String:
	if words.size() <= 1:
		return "".join(words)
	return "%s and %s" % [", ".join(words.slice(0, words.size() - 1)), words[words.size() - 1]]
