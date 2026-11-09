class_name Knowledge
extends RefCounted
## What one colonist believes about the colony, as opposed to what is true.
##
## Each fact remembers how it was learned: "seen" (with their own eyes) or "heard" (someone told
## them, with that person's name). Facts carry the time the information was *originally* observed,
## so hearsay passed along a chain keeps getting older. Recall blurs with age: the older a memory,
## the further off the remembered amount can be (misremembering has a natural reason). Facts that
## are old enough fade to "no idea".
##
## Trust rules (how much hearsay counts against what you already believe):
##   * fresher information wins,
##   * your own eyes beat hearsay of the same age (hearsay is treated as HEARSAY_PENALTY_H older),
##   * people you like are believed more: a stranger's news is treated up to DISTRUST_H older still.
## Hooks for exaggeration and lies exist (`tell(..., distort)`) but nothing uses them yet: colonists
## don't have motives for lying until there's competition or self-interest in the game.

const FORGET_AFTER_H := 72.0
const HEARSAY_PENALTY_H := 1.0
const DISTRUST_H := 4.0
const STOCK_KEYS := ["food", "water", "wood", "stone"]

var owner_id: int = 0
var facts: Dictionary = {}     # key -> {v: float, t: float, src: "seen"|"heard", from: String}
var places: Dictionary = {}    # resource node id -> {type, cell: Vector2i, t, src: "seen"|"heard", from, amt}
var seen := PackedByteArray()  # land this colonist has seen with their own eyes (y * size + x)


func _init(p_owner: int = 0) -> void:
	owner_id = p_owner


func observe(key: String, value: float, now: float) -> void:
	facts[key] = {"v": value, "t": now, "src": "seen", "from": ""}


## Someone told us `value` for `key`, which they learned at time `t_source`. Returns true if it
## changed what we believe (so the caller can make a memory of it).
func hear(key: String, value: float, t_source: float, from: String, trust: float, now: float) -> bool:
	var penalty := HEARSAY_PENALTY_H + DISTRUST_H * (1.0 - clampf(trust, 0.0, 1.0))
	var new_age := (now - t_source) / 60.0 + penalty
	var old: Dictionary = facts.get(key, {})
	if not old.is_empty():
		var old_age := (now - float(old["t"])) / 60.0 + (0.0 if old["src"] == "seen" else HEARSAY_PENALTY_H)
		if old_age <= new_age:
			return false
	var changed := old.is_empty() or absf(float(old["v"]) - value) > maxf(5.0, 0.2 * absf(value))
	facts[key] = {"v": value, "t": t_source, "src": "heard", "from": from}
	return changed


func knows(key: String, now: float) -> bool:
	return facts.has(key) and (now - float(facts[key]["t"])) / 60.0 < FORGET_AFTER_H


func age_h(key: String, now: float) -> float:
	return (now - float(facts[key]["t"])) / 60.0 if facts.has(key) else INF


## What they'd say if asked right now: the remembered value, blurred by age. Deterministic per
## fact (it doesn't flicker), and it's what gets passed on when they tell someone.
func recall(key: String, now: float) -> float:
	if not facts.has(key):
		return -1.0
	var f: Dictionary = facts[key]
	var age := (now - float(f["t"])) / 60.0
	var sigma := clampf(age / 96.0, 0.0, 0.35) + (0.08 if f["src"] == "heard" else 0.0)
	var h := absi(hash("%d|%s|%d" % [owner_id, key, int(f["t"])]))
	var u := float(h % 10000) / 10000.0 * 2.0 - 1.0          # -1..1, fixed for this memory
	return maxf(0.0, roundf(float(f["v"]) * (1.0 + sigma * u)))


## Value to pass on in conversation. distort is the hook for exaggerating or lying later.
func tell(key: String, now: float, distort: float = 0.0) -> float:
	return recall(key, now) * (1.0 + distort)


## Short phrase for the state text Laya reads, e.g. "food ~60 (seen 3h ago)", "wood ? (no idea)".
func phrase(key: String, now: float) -> String:
	if not knows(key, now):
		return "%s ? (no idea)" % key
	var f: Dictionary = facts[key]
	var age := age_h(key, now)
	var when := "just now" if age < 1.0 else "%dh ago" % int(age)
	var how := "seen %s" % when if f["src"] == "seen" else "%s said, %s" % [f["from"], when]
	return "%s ~%d (%s)" % [key, int(recall(key, now)), how]


## 0 = knows everything freshly, 1 = knows nothing (used by brains to decide whether to go check).
func uncertainty(now: float) -> float:
	var u := 0.0
	for k in STOCK_KEYS:
		u += 1.0 if not knows(k, now) else clampf(age_h(k, now) / 24.0, 0.0, 1.0)
	return u / STOCK_KEYS.size()


## ---- Places: where resources are --------------------------------------------------

## `amount`: how much they saw there (-1 = didn't notice).
func see_place(node_id: int, type: String, cell: Vector2i, now: float, amount: float = -1.0) -> void:
	places[node_id] = {"type": type, "cell": cell, "t": now, "src": "seen", "from": "", "amt": amount}


## Told where something is (and roughly how much the teller remembers there). Fresher news replaces
## older news; their own recent sighting wins.
func hear_place(node_id: int, type: String, cell: Vector2i, t_source: float, from: String, amount: float = -1.0) -> bool:
	if places.has(node_id) and float(places[node_id]["t"]) >= t_source:
		return false
	var is_new := not places.has(node_id)
	places[node_id] = {"type": type, "cell": cell, "t": t_source, "src": "heard", "from": from, "amt": amount}
	return is_new


## Places of a type they believe are worth going to: something was left there when last seen (or
## they don't know how much), or it regrows and they last saw it long enough ago.
func usable_places_of(type: String, from: Vector2i, now: float, regrows: bool) -> Array:
	return places_of(type, from).filter(func(p: Dictionary) -> bool:
		var e: Dictionary = places[p["id"]]
		var amt := float(e.get("amt", -1.0))
		return amt < 0.0 or amt >= 1.0 or (regrows and (now - float(e["t"])) / 60.0 > 24.0))


func forget_place(node_id: int) -> void:
	places.erase(node_id)


func knows_place(node_id: int) -> bool:
	return places.has(node_id)


## Known places of a type, nearest first: [{id, cell, d}].
func places_of(type: String, from: Vector2i) -> Array:
	var out := []
	for nid in places:
		var p: Dictionary = places[nid]
		if p["type"] == type:
			out.append({"id": nid, "cell": p["cell"], "d": Vector2(p["cell"] - from).length()})
	out.sort_custom(func(a, b) -> bool: return a["d"] < b["d"])
	return out


func to_dict() -> Dictionary:
	return facts.duplicate(true)
