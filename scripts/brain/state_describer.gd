class_name StateDescriber
extends RefCounted
## Turns a colonist's situation into short plain text for Laya (and the decision log).
## Laya's English checkpoint reads ~320 tokens of state, so keep this compact.
## Bump VERSION whenever the format changes so logged data stays comparable.

const VERSION := "v2"   # v2: stock is what the colonist believes (seen/heard, with age); requests
const MAX_MEMORIES := 3


func describe(sim: Sim, c: Colonist) -> String:
	var parts: PackedStringArray = []
	var trait_texts: PackedStringArray = []
	for t in c.traits:
		trait_texts.append(str(sim.content.traits.get(t, {}).get("text", t)))
	parts.append("%s is a%s %s colonist, %s." % [c.name, "n" if c.is_adult() else "", c.life_stage(), ", ".join(trait_texts)])
	parts.append("It is day %d, %s." % [sim.day(), _part_of_day(sim)])

	var need_words: PackedStringArray = []
	for need_id in sim.content.needs:
		need_words.append(sim.need_word(c, need_id))
	parts.append("%s is %s." % [c.name, _join_and(need_words)])

	# Not the true stock: what this colonist believes, and how they know it.
	var st: PackedStringArray = []
	for k in Knowledge.STOCK_KEYS:
		st.append(c.knowledge.phrase(k, sim.clock))
	parts.append("%s thinks the stores hold: %s." % [c.name, ", ".join(st)])

	var built := {}
	var sites: PackedStringArray = []
	for b: Building in sim.buildings.values():
		var lbl: String = b.def().get("label", b.type)
		if b.complete:
			built[lbl] = int(built.get(lbl, 0)) + 1
		else:
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
	return "%s, %s" % [rel, "nearby" if dist < 8.0 else "far away"]


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
