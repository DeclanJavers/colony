extends SceneTree
## How often do settled colonists (not walking) stand on top of each other?
##   godot --headless --path . -s res://tests/overlap_check.gd -- --days 5 --seed 1234
## Counts pairs closer than 0.3 tiles, sampled every 10 sim minutes, split by activity.

func _initialize() -> void:
	var args := OS.get_cmdline_user_args()
	var days := int(args[args.find("--days") + 1]) if args.has("--days") else 5
	var seed_v := int(args[args.find("--seed") + 1]) if args.has("--seed") else 1234
	var sim := Sim.new()
	sim.register_brain(UtilityBrain.new(), true)
	sim.setup(seed_v)
	var samples := 0
	var pairs := {}
	var end := sim.clock + days * 1440.0
	while sim.clock < end:
		sim.tick(1.0)
		if int(sim.clock) % 10 != 0:
			continue
		samples += 1
		var settled := []
		for c: Colonist in sim.colonists.values():
			if c.action != null and c.action.path.is_empty() and c.action.phase != "walking":
				settled.append(c)
		for i in settled.size():
			for j in range(i + 1, settled.size()):
				var a: Colonist = settled[i]
				var b: Colonist = settled[j]
				if a.pos.distance_to(b.pos) < 0.3:
					var k := "%s+%s" % [a.action.def["id"], b.action.def["id"]]
					pairs[k] = int(pairs.get(k, 0)) + 1
	var total := 0
	for k in pairs:
		total += int(pairs[k])
	print("OVERLAPS %d over %d samples" % [total, samples])
	var keys := pairs.keys()
	keys.sort_custom(func(x, y) -> bool: return pairs[x] > pairs[y])
	for k in keys.slice(0, 8):
		print("  %-28s %d" % [k, pairs[k]])
	quit()
