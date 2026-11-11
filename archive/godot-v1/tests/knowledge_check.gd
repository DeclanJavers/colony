extends SceneTree
## Checks that colonists' knowledge and trust behave as designed.
##   godot --headless --path . -s res://tests/knowledge_check.gd
## Prints one line per check and RESULT PASS / FAIL.

var fails := 0


func check(name: String, ok: bool, detail: String = "") -> void:
	print("%s  %s%s" % ["ok  " if ok else "FAIL", name, (" (" + detail + ")") if detail != "" else ""])
	if not ok:
		fails += 1


func _initialize() -> void:
	var now := 10000.0
	var k := Knowledge.new(1)

	k.observe("food", 50.0, now)
	check("fresh sight is remembered exactly", k.recall("food", now) == 50.0, str(k.recall("food", now)))

	var old_blur: float = absf(k.recall("food", now + 60.0 * 48.0) - 50.0)
	check("old memories blur, within bounds", old_blur <= 50.0 * 0.43, "off by %d after 48h" % old_blur)

	check("forgotten after %dh" % Knowledge.FORGET_AFTER_H, not k.knows("food", now + 60.0 * (Knowledge.FORGET_AFTER_H + 1)))

	var k2 := Knowledge.new(2)
	k2.observe("wood", 40.0, now)
	check("own eyes beat hearsay of the same age",
		not k2.hear("wood", 90.0, now, "Tam", 1.0, now) and k2.recall("wood", now) == 40.0)

	k2.observe("stone", 10.0, now - 60.0 * 10.0)   # saw it 10h ago
	check("fresher news from a friend wins",
		k2.hear("stone", 70.0, now - 60.0, "Tam", 0.9, now) and int(k2.facts["stone"]["v"]) == 70)

	k2.observe("water", 10.0, now - 60.0 * 3.0)    # saw it 3h ago
	check("a stranger's slightly fresher news doesn't override what you saw",
		not k2.hear("water", 60.0, now - 60.0 * 1.0, "Stranger", 0.1, now))

	check("hearsay keeps the original time (chains age)", float(k2.facts["stone"]["t"]) == now - 60.0)

	# Conversations in the sim: news spreads with its original timestamp; asks become requests.
	var sim := Sim.new()
	sim.register_brain(UtilityBrain.new(), true)
	sim.setup(5)
	var people := sim.colonists.values()
	var a: Colonist = people[0]
	var b: Colonist = people[1]
	sim.stock["food"] = 7.0
	for r in ["water", "wood", "stone"]:
		sim.stock[r] = 200.0
	sim.observe_stock(a)
	var t_seen := float(a.knowledge.facts["food"]["t"])
	b.knowledge.facts.erase("food")
	sim.converse(a, b, "chat")
	check("talking passes news on", b.knowledge.knows("food", sim.clock) and int(b.knowledge.facts["food"]["v"]) == 7,
		str(b.knowledge.facts.get("food", {})))
	check("passed-on news keeps when it was seen", float(b.knowledge.facts["food"]["t"]) == t_seen)

	sim.converse(a, b, "ask", "gather_food")
	var reqs := sim.active_requests(b)
	check("asking creates a request", not reqs.is_empty() and reqs[-1]["action"] == "gather_food" and reqs[-1]["from"] == a.name)
	check("believed shortage is noticed", sim.believed_shortage(a) == "food", sim.believed_shortage(a))

	print("RESULT ", "PASS" if fails == 0 else "FAIL (%d)" % fails)
	quit(0 if fails == 0 else 1)
