extends SceneTree
## Headless fast-forward run. Usage:
##   godot --headless --path . -s res://tests/run_headless.gd -- --days 10 --seed 1234 --brain utility --log 1
## Prints a summary per day and PASS/FAIL against the step-1 goal:
##   the founders survive and the colony builds a well.

func _initialize() -> void:
	var args := _parse(OS.get_cmdline_user_args())
	var days := int(args.get("days", "10"))
	var seed_v := int(args.get("seed", "1234"))
	var verbose: bool = args.get("verbose", "0") == "1"

	var sim := Sim.new()
	sim.register_brain(UtilityBrain.new(args.get("pick", "argmax")), true)
	sim.setup(seed_v)

	var log: DecisionLog = null
	if args.get("log", "1") == "1":
		log = DecisionLog.new()
		log.attach(sim, "headless")

	sim.events.day_passed.connect(func(_d: int) -> void: print("DAY ", JSON.stringify(sim.summary())))
	sim.events.colonist_died.connect(func(c: Colonist, cause: String) -> void:
		print("  [day %d %02d:00] %s died of %s (age %.1f)" % [sim.day(), sim.hour(), c.name, cause, c.age_days()]))
	sim.events.colonist_born.connect(func(c: Colonist) -> void:
		if not c.parent_ids.is_empty():
			print("  [day %d %02d:00] %s was born" % [sim.day(), sim.hour(), c.name]))
	sim.events.building_completed.connect(func(b: Building) -> void:
		print("  [day %d %02d:00] built %s" % [sim.day(), sim.hour(), b.type]))
	sim.events.couple_formed.connect(func(a: Colonist, b: Colonist) -> void:
		print("  [day %d %02d:00] couple: %s + %s" % [sim.day(), sim.hour(), a.name, b.name]))
	sim.events.tier_unlocked.connect(func(t: int) -> void:
		print("  [day %d %02d:00] unlocked tier %d" % [sim.day(), sim.hour(), t]))
	if verbose:
		sim.events.decision_made.connect(func(c: Colonist, _ctx: Dictionary, d: Dictionary, began: bool) -> void:
			print("    %s -> %s %s" % [c.name, d["action"], "" if began else "(FAILED)"]))

	var founders := sim.alive_count()
	var end := sim.clock + days * 1440.0
	var t0 := Time.get_ticks_msec()
	var action_counts := {}
	sim.events.action_started.connect(func(_c: Colonist, a: ActionBase) -> void:
		action_counts[a.def["id"]] = int(action_counts.get(a.def["id"], 0)) + 1)
	while sim.clock < end and sim.alive_count() > 0:
		sim.tick(1.0)
	var ms := Time.get_ticks_msec() - t0

	print("FINAL ", JSON.stringify(sim.summary()))
	print("ACTIONS ", JSON.stringify(action_counts))
	print("sim %d days in %d ms (%.1f days/sec)" % [days, ms, days / maxf(ms / 1000.0, 0.001)])
	if log != null:
		log.close()
		print("decision log: %s (%d lines)" % [log.path, log.written])
	var pass_ok := sim.alive_count() >= founders and sim.count_of("well", false) >= 1
	print("RESULT ", "PASS" if pass_ok else "FAIL")
	quit(0 if pass_ok else 1)


func _parse(argv: PackedStringArray) -> Dictionary:
	var out := {}
	var i := 0
	while i < argv.size():
		var a := argv[i]
		if a.begins_with("--"):
			var key := a.substr(2)
			if i + 1 < argv.size() and not argv[i + 1].begins_with("--"):
				out[key] = argv[i + 1]
				i += 1
			else:
				out[key] = "1"
		i += 1
	return out
