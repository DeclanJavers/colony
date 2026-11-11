extends SceneTree
## Headless fast-forward run. Usage:
##   godot --headless --path . -s res://tests/run_headless.gd -- --days 10 --seed 1234 --brain utility --log 1
## --brain laya runs the colony on Laya through the brain server (start brain_server/run.sh first).
## Laya answers asynchronously, so that mode advances the sim frame by frame and waits whenever a
## colonist has been thinking longer than decision.max_lag_minutes, exactly like the game does.
## Prints a summary per day and PASS/FAIL against the step-1 goal:
##   the founders survive and the colony builds a well.

const TICKS_PER_FRAME := 240

var sim: Sim
var log: DecisionLog
var laya: LayaClient
var founders := 0
var end_clock := 0.0
var t0 := 0
var days := 10
var max_lag := 15.0
var action_counts := {}
var brain_counts := {}
var metrics := ColonyMetrics.new()
var _done := false


func _initialize() -> void:
	var args := _parse(OS.get_cmdline_user_args())
	days = int(args.get("days", "10"))
	var seed_v := int(args.get("seed", "1234"))
	var verbose: bool = args.get("verbose", "0") == "1"
	var brain: String = args.get("brain", "utility")
	var content := Content.get_instance()
	max_lag = float(content.cfg("decision.max_lag_minutes", 15))

	sim = Sim.new()
	sim.register_brain(UtilityBrain.new(args.get("pick", "argmax")), brain == "utility")
	if brain == "laya":
		laya = LayaClient.new(String(content.cfg("brain_server_url", "http://127.0.0.1:8765")))
		root.add_child(laya)
		sim.register_brain(LayaBrain.new(laya, args.get("pick", "argmax")), true)
		sim.teacher_id = "utility"
	sim.setup(seed_v)

	if args.get("log", "1") == "1":
		log = DecisionLog.new()
		log.attach(sim, "headless_" + brain)

	sim.events.day_passed.connect(func(_d: int) -> void:
		var extra := ""
		if laya != null:
			extra = "  [laya %d answered, %d errors, %.0f ms/decision, %.0fs elapsed]" % [laya.answered, laya.errors, laya.avg_latency_ms, (Time.get_ticks_msec() - t0) / 1000.0]
		print("DAY ", JSON.stringify(sim.summary()), extra))
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

	# --watcher-requests H: every H sim hours, a random colonist hears "the Watcher" ask for a job,
	# like a player chatting would. Used for training data so the brain learns to weigh requests.
	var wr := float(args.get("watcher-requests", "0"))
	if wr > 0.0:
		var jobs := ["gather_food", "fetch_water", "gather_wood", "gather_stone", "farm", "build", "check_stores"]
		var rr := RandomNumberGenerator.new()
		rr.seed = seed_v * 31 + 7
		sim.events.hour_passed.connect(func(_d: int, h: int) -> void:
			if h % int(wr) != 0 or sim.colonists.is_empty():
				return
			var people := sim.colonists.values()
			var who: Colonist = people[rr.randi() % people.size()]
			if who.is_adult():
				sim.add_request(who, "the Watcher", jobs[rr.randi() % jobs.size()], 0.8))

	metrics.attach(sim)
	founders = sim.alive_count()
	end_clock = sim.clock + days * 1440.0
	t0 = Time.get_ticks_msec()
	sim.events.action_started.connect(func(_c: Colonist, a: ActionBase) -> void:
		action_counts[a.def["id"]] = int(action_counts.get(a.def["id"], 0)) + 1)
	sim.events.decision_made.connect(func(_c: Colonist, _ctx: Dictionary, d: Dictionary, _b: bool) -> void:
		brain_counts[d.get("action", "")] = int(brain_counts.get(d.get("action", ""), 0)) + 1)
	if laya == null:
		while sim.clock < end_clock and sim.alive_count() > 0:
			sim.tick(1.0)
		_finish()


## Laya mode: advance a slice of the sim each frame, pausing while anyone waits on Laya too long.
func _process(_delta: float) -> bool:
	if laya == null or _done:
		return _done
	for i in TICKS_PER_FRAME:
		if sim.clock >= end_clock or sim.alive_count() == 0:
			_finish()
			return true
		if sim.oldest_thinking_minutes() > max_lag:
			break
		sim.tick(1.0)
	return false


func _finish() -> void:
	if _done:
		return
	_done = true
	var ms := Time.get_ticks_msec() - t0
	print("FINAL ", JSON.stringify(sim.summary()))
	print("METRICS ", JSON.stringify(metrics.report()))
	print("ACTIONS ", JSON.stringify(action_counts))
	print("CHOICES ", JSON.stringify(brain_counts))
	print("failed decisions: %d of %d" % [sim.stats.get("failed_decisions", 0), sim.stats.get("decisions", 0)])
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
