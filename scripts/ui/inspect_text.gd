class_name InspectText
extends RefCounted
## Text for the inspector panel: one page per kind of thing you can click on.
## Pages use BBCode with links: [url=c:ID] colonist (alive or a grave), [url=b:ID] building,
## [url=n:ID] resource node, [url=t:X,Y] tile, [url=colony] colony overview. The inspector
## opens whatever a link points at and moves the camera there.

const GREY := "#aaaaaa"


## ---- Shared helpers ---------------------------------------------------------------------------

static func link(t: Variant) -> String:
	if t is Colonist:
		var c := t as Colonist
		return "[url=c:%d]%s[/url]%s" % [c.id, c.name, "" if c.alive() else " (dead)"]
	if t is Building:
		var b := t as Building
		return "[url=b:%d]%s[/url]" % [b.id, label_of(b)]
	if t is ResNode:
		var n := t as ResNode
		return "[url=n:%d]%s[/url]" % [n.id, String(n.type).replace("_", " ")]
	if t is Vector2i:
		return "[url=t:%d,%d]ground (%d, %d)[/url]" % [t.x, t.y, t.x, t.y]
	return str(t)


static func label_of(b: Building) -> String:
	var l: String = String(b.def().get("label", b.type))
	l = l.left(1).to_upper() + l.substr(1)
	return l if b.complete else "%s site" % l


static func when(sim: Sim, t: float) -> String:
	if t < 0.0:
		return "from the start"
	var ago := (sim.clock - t) / 60.0
	var stamp := "day %d %02d:%02d" % [int(t / 1440.0), int(fmod(t, 1440.0) / 60.0), int(fmod(t, 60.0))]
	if ago < 1.0:
		return "%s (just now)" % stamp
	if ago < 48.0:
		return "%s (%d h ago)" % [stamp, int(ago)]
	return "%s (%.1f days ago)" % [stamp, ago / 24.0]


static func gray(s: String) -> String:
	return "[color=%s]%s[/color]" % [GREY, s]


## The thing a colonist's current action is aimed at (Building, ResNode, Colonist, Vector2i or null).
static func action_target(a: ActionBase) -> Variant:
	if a == null:
		return null
	for key in ["site", "farm", "other"]:
		var v = a.get(key)
		if v != null:
			return v
	var src = a.get("source")
	if src is Dictionary and not (src as Dictionary).is_empty():
		return src.get("ref")
	if a.goal_cell.x >= 0:
		return a.goal_cell
	return null


## Equality that doesn't error when comparing different kinds (a tile vs a building).
static func same(a: Variant, b: Variant) -> bool:
	return typeof(a) == typeof(b) and a == b


## Living colonists whose current action targets t.
static func working_on(sim: Sim, t: Variant) -> Array:
	var out := []
	for c: Colonist in sim.colonists.values():
		if c.action != null and same(action_target(c.action), t):
			out.append(c)
	return out


static func names(list: Array) -> String:
	var parts: PackedStringArray = []
	for c in list:
		parts.append(link(c))
	return ", ".join(parts) if not parts.is_empty() else gray("nobody")


## ---- Pages ----------------------------------------------------------------------------------------

static func building(sim: Sim, b: Building) -> String:
	var d := b.def()
	var L: PackedStringArray = []
	L.append(gray(String(d.get("description", ""))))
	if b.complete:
		L.append("Finished %s." % when(sim, b.completed_at))
	else:
		var left := b.work_needed - b.work_done
		var workers := working_on(sim, b)
		var eta := ""
		if not workers.is_empty():
			eta = ", about %.1f h left with %d working" % [left / 60.0 / workers.size(), workers.size()]
		L.append("[b]Under construction: %d%%[/b] (%d of %d work minutes%s)" % [int(b.progress() * 100), int(b.work_done), int(b.work_needed), eta])
	L.append("Started %s%s." % [when(sim, b.placed_at), (" by " + _name_link(sim, b.placed_by)) if b.placed_by != "" else ""])
	if not b.work_by.is_empty():
		var total := 0.0
		for k in b.work_by:
			total += float(b.work_by[k])
		var parts: PackedStringArray = []
		var keys := b.work_by.keys()
		keys.sort_custom(func(x, y) -> bool: return float(b.work_by[x]) > float(b.work_by[y]))
		for k in keys:
			parts.append("%s %d%%" % [_name_link(sim, k), roundi(100.0 * float(b.work_by[k]) / maxf(total, 0.001))])
		L.append("Built by: %s" % ", ".join(parts))
	L.append("Here now: %s" % names(working_on(sim, b)))

	var prov: Dictionary = d.get("provides", {})
	if not prov.is_empty():
		L.append("\n[b]What it does[/b]")
	if prov.has("storage"):
		L.append("Stores up to %d. Colony storage: %d of %d used." % [int(prov["storage"]), int(sim.stock_total()), int(sim.storage_capacity())])
	if prov.has("beds"):
		var occ: PackedStringArray = []
		for cid in b.beds:
			var o := sim.find_colonist(int(cid)) if int(cid) >= 0 else null
			occ.append(link(o) if o != null else gray("free"))
		L.append("Beds: %s" % ", ".join(occ))
	if prov.has("sleep_quality"):
		L.append("Sleep here is %.1fx as restful." % float(prov["sleep_quality"]))
	if prov.has("rest_spot"):
		L.append("Rest spot: resting or sleeping here is %.2fx as good." % float(prov["rest_spot"]))
	if prov.has("water_source"):
		L.append("Water source: colonists drink and fetch water here.")
	if prov.has("farm"):
		var f: Dictionary = prov["farm"]
		var stage := b.farm_stage
		var extra := ""
		if stage == "growing":
			extra = ", %d%% grown, ripe in about %.0f h" % [int(b.farm_growth * 100), float(f.get("grow_hours", 36)) * (1.0 - b.farm_growth)]
		L.append("Field: [b]%s[/b]%s. Yields %d food per harvest; harvested %d times." % [stage, extra, int(f.get("yield", 20)), b.harvests])
		var who := sim.find_colonist(b.reserved_by) if b.reserved_by >= 0 else null
		if who != null:
			L.append("Being worked by %s." % link(who))
	if prov.has("colony"):
		var parts: PackedStringArray = []
		for k in prov["colony"]:
			parts.append("%s x%.2f" % [String(k).replace("_", " "), float(prov["colony"][k])])
		L.append("Colony bonus: %s" % ", ".join(parts))
	var cost: Dictionary = d.get("cost", {})
	if not cost.is_empty():
		var parts: PackedStringArray = []
		for r in cost:
			parts.append("%d %s" % [int(cost[r]), r])
		L.append("\n%s" % gray("Cost %s · tier %d · %dx%d tiles at (%d, %d)" % [", ".join(parts), int(d.get("tier", 0)), b.size.x, b.size.y, b.cell.x, b.cell.y]))
	return "\n".join(L)


static func node(sim: Sim, n: ResNode) -> String:
	var d := n.def()
	var L: PackedStringArray = []
	L.append("[b]%d of %d %s left[/b] (%d%%)" % [int(n.amount), int(n.max_amount), d.get("resource", "?"), int(100.0 * n.amount / maxf(n.max_amount, 1.0))])
	L.append("A trip takes about %d min of work and brings back up to %d." % [int(d.get("work_minutes", 20)), int(d.get("per_trip", 6))])
	var regrow := float(d.get("regrow_per_hour", 0))
	L.append("Regrows %.1f per hour." % regrow if regrow > 0 else "Doesn't regrow: it's gone once used up.")
	L.append("Being worked by: %s" % names(working_on(sim, n)))
	L.append(gray("Tile (%d, %d)" % [n.cell.x, n.cell.y]))
	return "\n".join(L)


static func grave(sim: Sim, c: Colonist) -> String:
	var L: PackedStringArray = []
	L.append("[b]%s died of %s[/b] %s, aged %.1f days." % [c.name, c.cause_of_death, when(sim, c.died_at), c.age_days()])
	L.append("Traits: %s" % ", ".join(c.traits))
	var fam: PackedStringArray = []
	if c.partner_id >= 0:
		var p := sim.find_colonist(c.partner_id)
		if p != null:
			fam.append("partner " + link(p))
	for pid in c.parent_ids:
		var p := sim.find_colonist(int(pid))
		if p != null:
			fam.append("parent " + link(p))
	for o in sim.colonists.values() + sim.graveyard:
		if (o as Colonist).parent_ids.has(c.id):
			fam.append("child " + link(o))
	if not fam.is_empty():
		L.append("Family: %s" % ", ".join(fam))
	L.append("\n[b]Last memories[/b]")
	var mems := c.memories.duplicate()
	mems.reverse()
	for m in mems.slice(0, 8):
		L.append("· %s" % m["text"])
	return "\n".join(L)


static func tile(sim: Sim, cell: Vector2i) -> String:
	var kind := "grass"
	match sim.map.tile(cell):
		WorldMap.Tile.WATER:
			kind = "river (can't cross; colonists drink at the shore)"
		WorldMap.Tile.FORD:
			kind = "shallow ford (the river can be crossed here)"
	var L: PackedStringArray = ["Ground: %s" % kind,
		"Walkable: %s" % ("yes" if sim.map.is_walkable(cell) else "no")]
	var here := []
	for c: Colonist in sim.colonists.values():
		if c.cell == cell:
			here.append(c)
	if not here.is_empty():
		L.append("Standing here: %s" % names(here))
	var heading := []
	for c: Colonist in sim.colonists.values():
		if c.action != null and c.action.goal_cell == cell:
			heading.append(c)
	if not heading.is_empty():
		L.append("Heading here: %s" % names(heading))
	return "\n".join(L)


static func colony(sim: Sim, runner: SimRunner) -> String:
	var L: PackedStringArray = []
	var adults := 0
	var couples := 0
	for c: Colonist in sim.colonists.values():
		adults += 1 if c.is_adult() else 0
		couples += 1 if c.partner_id >= 0 else 0
	var deaths: Dictionary = sim.stats.get("deaths", {})
	var death_parts: PackedStringArray = []
	for k in deaths:
		death_parts.append("%d %s" % [int(deaths[k]), k])
	L.append("[b]Day %d[/b], %s. Tier %d." % [sim.day(), "night" if sim.is_night() else "day", sim.unlocked_tier])
	L.append("%d people: %d adults, %d children, %d couples. %d births, %s." % [
		sim.alive_count(), adults, sim.alive_count() - adults, couples / 2, int(sim.stats.get("births", 0)),
		("deaths: " + ", ".join(death_parts)) if not death_parts.is_empty() else "no deaths"])

	L.append("\n[b]Stock[/b] %s" % gray("(change over the last day)"))
	var past := runner.snapshot_hours_ago(24)
	for r in sim.stock:
		var now := float(sim.stock[r])
		var delta := ""
		if not past.is_empty():
			var dv := now - float(past["stock"].get(r, now))
			delta = " [color=%s]%+d[/color]" % ["#9be29b" if dv >= 0 else "#ff8a80", int(dv)]
		L.append("%s: %d%s" % [String(r).capitalize(), int(now), delta])
	L.append(gray("Storage %d of %d, free beds %d." % [int(sim.stock_total()), int(sim.storage_capacity()), sim.free_beds()]))

	L.append("\n[b]Next tier[/b]")
	var next_t: Dictionary = {}
	for t: Dictionary in sim.content.tiers:
		if int(t["tier"]) == sim.unlocked_tier + 1:
			next_t = t
	if next_t.is_empty():
		L.append("Every tier is unlocked.")
	else:
		var req: Dictionary = next_t.get("requires", {})
		var parts: PackedStringArray = []
		for bt in req:
			var have := sim.count_of(bt, false)
			var col := "#9be29b" if have >= int(req[bt]) else "#ffe082"
			parts.append("[color=%s]%s %d/%d[/color]" % [col, sim.content.buildings[bt].get("label", bt), have, int(req[bt])])
		L.append("Tier %d needs: %s" % [int(next_t["tier"]), ", ".join(parts)])
		var unlocks: PackedStringArray = []
		for bt in sim.content.buildings:
			if int(sim.content.buildings[bt].get("tier", 0)) == int(next_t["tier"]):
				unlocks.append(sim.content.buildings[bt].get("label", bt))
		L.append(gray("Unlocks: %s" % ", ".join(unlocks)))

	L.append("\n[b]Buildings[/b]")
	var counts := {}
	var sites := []
	for b: Building in sim.buildings.values():
		if b.complete:
			var l := label_of(b)
			counts[l] = int(counts.get(l, 0)) + 1
		else:
			sites.append(b)
	var parts2: PackedStringArray = []
	for k in counts:
		parts2.append("%d %s" % [counts[k], k])
	L.append(", ".join(parts2))
	if not sites.is_empty():
		var sp: PackedStringArray = []
		for b in sites:
			sp.append("%s %d%%" % [link(b), int(b.progress() * 100)])
		L.append("Being built: %s" % ", ".join(sp))

	L.append("\n[b]People[/b]")
	var people := sim.colonists.values()
	people.sort_custom(func(a, b) -> bool: return a.name < b.name)
	for c: Colonist in people:
		var worst := ""
		var worst_v := 101.0
		for need_id in sim.content.needs:
			var v := float(c.needs.get(need_id, 100))
			if v < worst_v:
				worst_v = v
				worst = sim.need_word(c, need_id)
		var col := "#dddddd" if worst_v > 25 else "#ff8a80"
		var doing := c.action.describe() if c.action != null else c.state
		L.append("%s [color=%s]· %s · %s[/color]" % [link(c), col, doing, worst])
	if not sim.graveyard.is_empty():
		var gp: PackedStringArray = []
		for g in sim.graveyard:
			gp.append(link(g))
		L.append(gray("Graves: ") + ", ".join(gp))

	L.append("\n[b]Brain[/b]")
	var s := "%d decisions, %d couldn't be carried out" % [int(sim.stats.get("decisions", 0)), int(sim.stats.get("failed_decisions", 0))]
	if runner.laya != null:
		s += ". " + runner.laya.status()
	var talks := runner.get_tree().root.find_child("ConversationLLM", true, false) as ConversationLLM
	if talks != null:
		s += ". " + talks.status()
	else:
		s += ". Brain: %s" % runner.brain_id
	L.append(s)
	return "\n".join(L)


static func _name_link(sim: Sim, name: String) -> String:
	for c in sim.colonists.values() + sim.graveyard:
		if (c as Colonist).name == name:
			return link(c)
	return name
